use crate::config::Config;
use crate::conversation::message::{ActionRequiredData, Message, MessageContent, MessageMetadata};
use crate::conversation::{merge_consecutive_messages, Conversation};
use crate::prompt_template::render_template;
use crate::providers::base::Provider;
#[cfg(test)]
use crate::providers::base::{stream_from_single_message, MessageStream};
use anyhow::Result;
use gosling_providers::conversation::token_usage::ProviderUsage;
use gosling_providers::errors::ProviderError;
use gosling_providers::model::ModelConfig;
use gosling_providers::retry::{retry_operation, RetryConfig};
use indoc::indoc;
use rmcp::model::Role;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::info;
use tracing::log::warn;

pub mod block;
pub mod budget;
pub mod memory;
pub mod packet;
pub mod policy;
pub mod selector;
pub mod summarizer;
pub mod telemetry;

pub use block::{ContextBlock, ContextPriority, ContextSlot};
pub use budget::ContextBudgetPolicy;
pub use memory::{FileMemorySource, MemoryItem, MemoryQuery, MemorySource, NoopMemorySource};
pub use packet::{
    resolve_provider_input, ContextBuildRequest, ContextManager, ContextPacket,
    ContextPacketMetadata, ContextStrategy,
};
pub use policy::{context_manager_mode, ContextManagerMode};
pub use summarizer::{summarizer_mode, PendingSummary, SummarizerMode, SummarizerTarget};

pub const DEFAULT_COMPACTION_THRESHOLD: f64 = 0.8;

/// How far below `GOSLING_AUTO_COMPACT_THRESHOLD` auto-compaction targets when
/// it fires, expressed as a fraction of the context window. E.g. threshold
/// 0.8 with the 0.15 default lands auto-compaction at 0.65 usage in a single
/// pass, regardless of how far past 0.8 usage had climbed before the check
/// ran — see `auto_compact_reduction_budget`.
pub const DEFAULT_AUTO_COMPACT_REDUCTION: f64 = 0.15;

const TOOLCALL_SUMMARIZATION_BATCH_SIZE: usize = 10;
const COMPACTION_MAX_INPUT_BYTES: usize = 192 * 1024;
const COMPACTION_MIN_INPUT_BYTES: usize = 24 * 1024;
const COMPACTION_MAX_INPUT_TOKENS: usize = 60_000;
const COMPACTION_SUMMARY_TARGET_CHARACTERS: usize = 12_000;
const COMPACTION_MAX_REDUCTION_ROUNDS: usize = 12;
const COMPACT_BAND_BASE_CHARACTERS: usize = 4_000;
const COMPACT_BAND_STEP_CHARACTERS: usize = 600;
const COMPACT_BAND_MIN_CHARACTERS: usize = 400;

fn tool_pair_summarization_enabled() -> bool {
    Config::global()
        .get_param::<bool>("GOSLING_TOOL_PAIR_SUMMARIZATION")
        .unwrap_or(true)
}

const DEFAULT_COMPACT_PROTECT_LAST_N_TURNS: usize = 10;

/// Number of most-recent real turns (a turn starts at a genuine user prompt,
/// not a tool response) that auto-compaction keeps verbatim instead of folding
/// into the summary. Without this, the exchange the user is actively replying
/// to could be compacted away just like everything older, leaving the agent
/// unable to resolve a direct follow-up ("that idea") without re-deriving it
/// from files. Turns older than this still get summarized, but with linearly
/// decreasing detail the further back they are — see `compaction_bands`.
fn compact_protect_last_n_turns() -> usize {
    Config::global()
        .get_param::<usize>("GOSLING_COMPACT_PROTECT_LAST_N_TURNS")
        .unwrap_or(DEFAULT_COMPACT_PROTECT_LAST_N_TURNS)
}

fn auto_compact_reduction() -> f64 {
    Config::global()
        .get_param::<f64>("GOSLING_AUTO_COMPACT_REDUCTION")
        .unwrap_or(DEFAULT_AUTO_COMPACT_REDUCTION)
}

/// A turn starts at an agent-visible user message that isn't itself a tool
/// response delivery (tool responses are represented as Role::User messages).
fn is_turn_start(msg: &Message) -> bool {
    msg.is_agent_visible()
        && matches!(msg.role, rmcp::model::Role::User)
        && !msg
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolResponse(_)))
}

#[derive(Debug)]
struct CompactionBand {
    start_idx: usize,
    end_idx: usize,
    target_characters: usize,
}

/// Partitions `messages[..compact_end]` into equal-width blocks of
/// `block_width_turns` turns, counting backward from `compact_end` (the
/// oldest block may be narrower). Each block's summarization budget decreases
/// linearly by `COMPACT_BAND_STEP_CHARACTERS` per block-distance from the
/// cutoff, floored at `COMPACT_BAND_MIN_CHARACTERS`, so a block right before
/// the protected tail keeps far more detail than one from deep history
/// instead of both being diluted evenly across one flat summary.
fn compaction_bands(
    turn_starts: &[usize],
    compact_end: usize,
    block_width_turns: usize,
) -> Vec<CompactionBand> {
    let eligible: Vec<usize> = turn_starts
        .iter()
        .copied()
        .filter(|&idx| idx < compact_end)
        .collect();
    if eligible.is_empty() {
        return Vec::new();
    }

    let block_width = block_width_turns.max(1);
    let mut bands = Vec::new();
    let mut remaining = eligible.len();
    let mut end_idx = compact_end;

    while remaining > 0 {
        let take = block_width.min(remaining);
        let start_turn = remaining - take;
        let start_idx = eligible[start_turn];
        let target_characters = COMPACT_BAND_BASE_CHARACTERS
            .saturating_sub(bands.len() * COMPACT_BAND_STEP_CHARACTERS)
            .max(COMPACT_BAND_MIN_CHARACTERS);
        bands.push(CompactionBand {
            start_idx,
            end_idx,
            target_characters,
        });
        end_idx = start_idx;
        remaining = start_turn;
    }

    bands.reverse();
    bands
}

/// Finds how far into the eligible region (oldest-first, up to `ceiling`)
/// auto-compaction needs to reach to remove roughly `tokens_to_remove` raw
/// tokens, so newer-but-still-eligible turns can be left untouched instead of
/// folding the whole region into a summary every time. Falls back to
/// `ceiling` (compact everything eligible, same as a `None` budget) when even
/// the full region doesn't cover the requested reduction.
///
/// Counts each turn's raw pre-summarization size rather than the net size
/// change (original minus the resulting summary), so this slightly
/// overshoots the requested reduction rather than undershoot it.
fn budget_capped_compact_end(
    messages: &[Message],
    turn_starts: &[usize],
    ceiling: usize,
    tokens_to_remove: usize,
    token_counter: &crate::token_counter::TokenCounter,
) -> usize {
    let eligible: Vec<usize> = turn_starts
        .iter()
        .copied()
        .filter(|&idx| idx < ceiling)
        .collect();

    let mut removed = 0usize;
    for (i, &start) in eligible.iter().enumerate() {
        let end = eligible.get(i + 1).copied().unwrap_or(ceiling);
        removed += token_counter.count_chat_tokens("", &messages[start..end], &[]);
        if removed >= tokens_to_remove {
            return end;
        }
    }
    ceiling
}

const CONVERSATION_CONTINUATION_TEXT: &str =
    "Your context was compacted. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context.";

const TOOL_LOOP_CONTINUATION_TEXT: &str =
    "Your context was compacted. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Continue calling tools as necessary to complete the task.";

const MANUAL_COMPACT_CONTINUATION_TEXT: &str =
    "Your context was compacted at the user's request. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context.";

pub fn compaction_failure_message(error: &dyn std::fmt::Display) -> String {
    format!(
        "Compaction did not complete: {error}\n\nYour original session is intact. You can switch providers and run /compact again, or start a new session with the essential context."
    )
}

#[derive(Serialize)]
struct SummarizeContext {
    messages: String,
    summary_target_characters: usize,
}

/// Compact messages by summarizing them
///
/// This function performs the actual compaction by summarizing messages and updating
/// their visibility metadata. It does not check thresholds - use `check_if_compaction_needed`
/// first to determine if compaction is necessary.
///
/// # Arguments
/// * `provider` - The provider to use for summarization
/// * `session_id` - The session to use for summarization
/// * `conversation` - The current conversation history
/// * `manual_compact` - If true, this is a manual compaction (don't preserve user message)
/// * `tokens_to_remove` - If `Some`, only the oldest slice of the eligible
///   (non-protected) region needed to remove roughly this many tokens is
///   folded into the summary; anything newer stays untouched for a future
///   pass. `None` collapses the whole eligible region as before — always the
///   case for `manual_compact`, and used by auto-compaction itself when it
///   needs a guaranteed full resolution (e.g. recovering from a hard context
///   overflow) rather than a soft trim. See `auto_compact_reduction_budget`.
///
/// # Returns
/// * A tuple containing:
///   - `Conversation`: The compacted messages
///   - `ProviderUsage`: Provider usage from summarization
pub async fn compact_messages(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    conversation: &Conversation,
    manual_compact: bool,
    tokens_to_remove: Option<usize>,
) -> Result<(Conversation, ProviderUsage)> {
    info!("Performing message compaction");

    let messages = conversation.messages();

    let has_text_only = |msg: &Message| {
        let has_text = msg
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::Text(_)));
        let has_tool_content = msg.content.iter().any(|c| {
            matches!(
                c,
                MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
            )
        });
        has_text && !has_tool_content
    };

    let extract_text = |msg: &Message| -> Option<String> {
        let text_parts: Vec<String> = msg
            .content
            .iter()
            .filter_map(|c| {
                if let MessageContent::Text(text) = c {
                    Some(text.text.clone())
                } else {
                    None
                }
            })
            .collect();

        if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        }
    };

    // Manual /compact intentionally summarizes everything the user asked to compact.
    // Auto-compaction protects the last few real turns (see compact_protect_last_n_turns)
    // so the exchange the user is actively replying to survives with full fidelity.
    let protect_last_n = if manual_compact {
        0
    } else {
        compact_protect_last_n_turns()
    };

    let turn_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| is_turn_start(msg))
        .map(|(idx, _)| idx)
        .collect();

    let protected_start = (protect_last_n > 0 && turn_starts.len() > protect_last_n)
        .then(|| turn_starts[turn_starts.len() - protect_last_n]);

    // Fallback for conversations too short to have `protect_last_n` full turns:
    // preserve just the most recent user text message, as before.
    let (preserved_user_message, is_most_recent) = if protected_start.is_none() && !manual_compact {
        let found_msg = messages.iter().enumerate().rev().find(|(_, msg)| {
            msg.is_agent_visible()
                && matches!(msg.role, rmcp::model::Role::User)
                && has_text_only(msg)
        });

        if let Some((idx, msg)) = found_msg {
            let is_last = idx == messages.len() - 1;
            (Some(msg.clone()), is_last)
        } else {
            (None, false)
        }
    } else {
        (None, false)
    };

    // `compact_end` is where the actual summary boundary falls this pass; it's
    // only ever less than `protected_start` when a reduction budget lets us
    // stop early, leaving newer-but-still-eligible turns untouched instead of
    // folding the entire eligible region into the summary every time.
    let compact_end = match protected_start {
        Some(ceiling) => Some(match tokens_to_remove {
            Some(budget) if budget > 0 => {
                let token_counter = crate::token_counter::shared_token_counter()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create token counter: {}", e))?;
                budget_capped_compact_end(
                    messages,
                    &turn_starts,
                    ceiling,
                    budget,
                    token_counter.as_ref(),
                )
            }
            _ => ceiling,
        }),
        None => None,
    };

    let messages_to_compact = match compact_end {
        Some(split) => &messages[..split],
        None => messages.as_slice(),
    };

    let bands = compact_end
        .map(|split| compaction_bands(&turn_starts, split, protect_last_n.max(1)))
        .unwrap_or_default();

    let (summary_message, summarization_usage) = if bands.is_empty() {
        do_compact(
            provider,
            model_config,
            session_id,
            messages_to_compact,
            COMPACTION_SUMMARY_TARGET_CHARACTERS,
        )
        .await?
    } else {
        let mut combined = String::new();
        let mut total_usage: Option<ProviderUsage> = None;
        for band in &bands {
            let (band_msg, band_usage) = do_compact(
                provider,
                model_config,
                session_id,
                &messages[band.start_idx..band.end_idx],
                band.target_characters,
            )
            .await?;
            combine_usage(&mut total_usage, band_usage);
            if let Some(text) = extract_text(&band_msg) {
                if !combined.is_empty() {
                    combined.push_str("\n\n");
                }
                combined.push_str(&text);
            }
        }
        (
            Message::user().with_text(combined),
            total_usage.expect("at least one band ran when bands is non-empty"),
        )
    };

    // Create the final message list with updated visibility metadata:
    // 1. Original messages become user_visible but not agent_visible
    // 2. Summary message becomes agent_visible but not user_visible
    // 3. Assistant messages to continue the conversation are also agent_visible but not user_visible
    let mut final_messages = Vec::new();

    for (idx, msg) in messages_to_compact.iter().enumerate() {
        let updated_metadata = if is_most_recent
            && idx == messages_to_compact.len() - 1
            && preserved_user_message.is_some()
        {
            // This is the most recent message and we're preserving it by adding a fresh copy
            MessageMetadata::invisible()
        } else {
            msg.metadata.clone().with_agent_invisible()
        };
        let updated_msg = msg.clone().with_metadata(updated_metadata);
        final_messages.push(updated_msg);
    }

    let summary_msg = summary_message.with_metadata(MessageMetadata::agent_only());

    let mut continuation_messages = vec![summary_msg];

    let tail_is_fresh_user_message = messages
        .last()
        .map(|m| is_turn_start(m) && has_text_only(m))
        .unwrap_or(false);

    let continuation_text = if manual_compact {
        MANUAL_COMPACT_CONTINUATION_TEXT
    } else if protected_start.is_some() {
        if tail_is_fresh_user_message {
            CONVERSATION_CONTINUATION_TEXT
        } else {
            TOOL_LOOP_CONTINUATION_TEXT
        }
    } else if is_most_recent {
        CONVERSATION_CONTINUATION_TEXT
    } else {
        TOOL_LOOP_CONTINUATION_TEXT
    };

    let continuation_msg = Message::assistant()
        .with_text(continuation_text)
        .with_metadata(MessageMetadata::agent_only());
    continuation_messages.push(continuation_msg);

    let (merged_continuation, _issues) = merge_consecutive_messages(continuation_messages);
    final_messages.extend(merged_continuation);

    if let Some(split) = protected_start {
        // When a reduction budget left `compact_end` short of `protected_start`,
        // the turns in between were never folded into the summary — splice them
        // back in verbatim so they stay part of the real conversation (and
        // become eligible for compaction on a future pass, once they've aged
        // further behind the protected tail).
        let untouched_start = compact_end.unwrap_or(split);
        if untouched_start < split {
            final_messages.extend(messages[untouched_start..split].iter().cloned());
        }

        // Keep the protected tail exactly as-is: real tool calls, attachments,
        // and all, rather than a reconstructed text-only stand-in.
        final_messages.extend(messages[split..].iter().cloned());
    } else if let Some(user_msg) = preserved_user_message {
        if let Some(text) = extract_text(&user_msg) {
            final_messages.push(
                Message::user()
                    .with_text(&text)
                    .with_metadata(user_msg.metadata.clone()),
            );
        }
    }

    Ok((
        Conversation::new_unvalidated(final_messages),
        summarization_usage,
    ))
}

/// Check if messages exceed the auto-compaction threshold
pub async fn check_if_compaction_needed(
    provider: &dyn Provider,
    conversation: &Conversation,
    threshold_override: Option<f64>,
    session: &crate::session::Session,
) -> Result<bool> {
    if provider.manages_own_context() {
        return Ok(false);
    }

    let config = Config::global();
    let threshold = threshold_override.unwrap_or_else(|| {
        config
            .get_param::<f64>("GOSLING_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD)
    });

    // Skip the tokenization pass entirely when auto-compact is disabled.
    if threshold <= 0.0 || threshold >= 1.0 {
        if threshold >= 1.0 {
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!(
                    "GOSLING_AUTO_COMPACT_THRESHOLD={} disables auto-compaction; use a value between 0 and 1 (or 0 to disable explicitly)",
                    threshold
                );
            }
        }
        return Ok(false);
    }

    let usage = resolve_context_usage(provider, conversation, session).await?;
    let usage_ratio = usage.current_tokens as f64 / usage.context_limit as f64;
    Ok(usage_ratio > threshold)
}

struct ContextUsage {
    context_limit: usize,
    current_tokens: usize,
}

/// Resolves the provider's real context limit (falling back to the
/// configured model when a session never persisted one — imports, fresh ACP
/// sessions — so canonical limits like a 1M-context model still apply) and
/// the conversation's current token usage, taking whichever of the stored
/// session usage or a fresh tokenization is higher (the stored value is
/// recorded before tool responses are added, so it can miss large tool
/// outputs).
async fn resolve_context_usage(
    provider: &dyn Provider,
    conversation: &Conversation,
    session: &crate::session::Session,
) -> Result<ContextUsage> {
    let config = Config::global();
    let model_config = session
        .model_config
        .clone()
        .or_else(|| {
            config.get_gosling_model().ok().and_then(|model| {
                crate::model_config::model_config_from_user_config(provider.get_name(), model).ok()
            })
        })
        .unwrap_or_else(|| ModelConfig::new("unknown"));
    let context_limit = provider
        .get_context_limit(&model_config)
        .await
        .unwrap_or_else(|_| model_config.context_limit());

    let token_counter = crate::token_counter::shared_token_counter()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create token counter: {}", e))?;

    let estimated_tokens = token_counter.count_chat_tokens("", conversation.messages(), &[]);

    let current_tokens = match session.usage.total_tokens {
        Some(stored) => (stored as usize).max(estimated_tokens),
        None => estimated_tokens,
    };

    Ok(ContextUsage {
        context_limit,
        current_tokens,
    })
}

/// Computes how many tokens auto-compaction should try to remove so the
/// conversation lands at `threshold - GOSLING_AUTO_COMPACT_REDUCTION` of the
/// context window in a single pass — regardless of how far past `threshold`
/// usage had already climbed when the check ran, rather than needing several
/// turns to crawl back under it. Returns `None` when the reduction is
/// disabled or misconfigured (`<= 0` or `>= threshold`), which tells the
/// caller to fall back to collapsing the whole eligible region as before.
///
/// `threshold_override`/`reduction_override` mirror `check_if_compaction_needed`'s
/// `threshold_override`: production callers pass `None` to read the real
/// `Config::global()` values, and tests pass explicit values to stay
/// independent of the operator's actual settings file.
pub async fn auto_compact_reduction_budget(
    provider: &dyn Provider,
    conversation: &Conversation,
    session: &crate::session::Session,
    threshold_override: Option<f64>,
    reduction_override: Option<f64>,
) -> Result<Option<usize>> {
    let threshold = threshold_override.unwrap_or_else(|| {
        Config::global()
            .get_param::<f64>("GOSLING_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD)
    });
    let reduction = reduction_override.unwrap_or_else(auto_compact_reduction);

    if reduction <= 0.0 || reduction >= threshold {
        return Ok(None);
    }

    let usage = resolve_context_usage(provider, conversation, session).await?;
    let target_tokens = (usage.context_limit as f64 * (threshold - reduction)) as usize;
    Ok(Some(usage.current_tokens.saturating_sub(target_tokens)))
}

fn filter_tool_pairs(messages: &[Message], remove_percent: u32) -> Vec<Message> {
    if remove_percent == 0 {
        return messages.to_vec();
    }

    let response_ids: HashSet<&str> = messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::ToolResponse(response) => Some(response.id.as_str()),
            _ => None,
        })
        .collect();
    let mut matched_ids = Vec::new();
    for content in messages.iter().flat_map(|message| &message.content) {
        if let MessageContent::ToolRequest(request) = content {
            if response_ids.contains(request.id.as_str())
                && !matched_ids.iter().any(|id| id == &request.id)
            {
                matched_ids.push(request.id.clone());
            }
        }
    }

    if matched_ids.is_empty() {
        return messages.to_vec();
    }

    let num_to_remove = ((matched_ids.len() * remove_percent as usize) / 100)
        .max(1)
        .min(matched_ids.len());

    let middle = matched_ids.len() / 2;
    let mut candidate_order: Vec<usize> = (0..matched_ids.len()).collect();
    candidate_order.sort_by_key(|&i| (i.abs_diff(middle), i));
    let ids_to_remove: HashSet<&str> = candidate_order[..num_to_remove]
        .iter()
        .map(|&i| matched_ids[i].as_str())
        .collect();

    messages
        .iter()
        .filter_map(|message| {
            let mut filtered = message.clone();
            filtered.content.retain(|content| match content {
                MessageContent::ToolRequest(request) => {
                    !ids_to_remove.contains(request.id.as_str())
                }
                MessageContent::ToolResponse(response) => {
                    !ids_to_remove.contains(response.id.as_str())
                }
                _ => true,
            });
            (!filtered.content.is_empty()).then_some(filtered)
        })
        .collect()
}

fn char_boundary_at_or_before(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn split_text_to_budget(
    text: &str,
    max_bytes: usize,
    max_tokens: usize,
    token_counter: &crate::token_counter::TokenCounter,
) -> Vec<String> {
    if text.len() <= max_bytes && token_counter.count_tokens(text) <= max_tokens {
        return vec![text.to_string()];
    }

    let mut segments = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        let mut end = char_boundary_at_or_before(remaining, max_bytes);
        while end > 0
            && token_counter.count_tokens(
                remaining
                    .get(..end)
                    .expect("end is adjusted to a UTF-8 character boundary"),
            ) > max_tokens
        {
            end = char_boundary_at_or_before(remaining, end * 3 / 4);
        }
        if end == 0 {
            end = remaining.chars().next().map(char::len_utf8).unwrap_or(0);
        }
        segments.push(
            remaining
                .get(..end)
                .expect("end is adjusted to a UTF-8 character boundary")
                .to_string(),
        );
        remaining = remaining
            .get(end..)
            .expect("end is adjusted to a UTF-8 character boundary");
    }

    if segments.len() > 1 && max_bytes > 512 && max_tokens > 128 {
        let segment_count = segments.len();
        for (index, segment) in segments.iter_mut().enumerate() {
            *segment = format!(
                "[Oversized message segment {} of {}]\n{}",
                index + 1,
                segment_count,
                segment
            );
        }
    }
    segments
}

fn pack_compaction_units(
    units: &[String],
    max_bytes: usize,
    max_tokens: usize,
    token_counter: &crate::token_counter::TokenCounter,
) -> Vec<String> {
    let payload_bytes = max_bytes.saturating_sub(512).max(1);
    let payload_tokens = max_tokens.saturating_sub(128).max(1);
    let expanded: Vec<String> = units
        .iter()
        .flat_map(|unit| split_text_to_budget(unit, payload_bytes, payload_tokens, token_counter))
        .collect();

    let mut chunks = Vec::new();
    let mut current = String::new();
    for unit in expanded {
        let separator = if current.is_empty() { "" } else { "\n\n" };
        let candidate = format!("{current}{separator}{unit}");
        if !current.is_empty()
            && (candidate.len() > max_bytes || token_counter.count_tokens(&candidate) > max_tokens)
        {
            chunks.push(current);
            current = unit;
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn combine_usage(total: &mut Option<ProviderUsage>, usage: ProviderUsage) {
    *total = Some(match total.take() {
        Some(existing) => existing.combine_with(&usage),
        None => usage,
    });
}

async fn summarize_compaction_chunk(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    system_prompt: &str,
    chunk: String,
) -> Result<(Message, ProviderUsage), ProviderError> {
    let summarization_request = vec![Message::user().with_text(chunk)];
    retry_operation(&RetryConfig::default(), || {
        crate::model_config::complete_fast(
            provider,
            model_config,
            session_id,
            system_prompt,
            &summarization_request,
            &[],
        )
    })
    .await
}

struct CompactionRequestContext<'a> {
    provider: &'a dyn Provider,
    model_config: &'a ModelConfig,
    session_id: &'a str,
    system_prompt: &'a str,
    max_tokens: usize,
    token_counter: &'a crate::token_counter::TokenCounter,
}

async fn reduce_compaction_units(
    request_context: &CompactionRequestContext<'_>,
    initial_units: Vec<String>,
    max_bytes: usize,
) -> Result<(Message, ProviderUsage), (ProviderError, Option<ProviderUsage>)> {
    let mut chunks = pack_compaction_units(
        &initial_units,
        max_bytes,
        request_context.max_tokens,
        request_context.token_counter,
    );
    let mut total_usage = None;

    for _ in 0..COMPACTION_MAX_REDUCTION_ROUNDS {
        let mut summaries = Vec::with_capacity(chunks.len());
        let is_final = chunks.len() == 1;
        let mut final_message = None;

        for chunk in chunks {
            let (mut response, mut usage) = summarize_compaction_chunk(
                request_context.provider,
                request_context.model_config,
                request_context.session_id,
                request_context.system_prompt,
                chunk.clone(),
            )
            .await
            .map_err(|error| (error, total_usage.take()))?;
            crate::providers::usage_estimator::ensure_usage_tokens(
                &mut usage,
                request_context.system_prompt,
                &[Message::user().with_text(chunk)],
                &response,
                &[],
            )
            .await
            .map_err(|error| {
                (
                    ProviderError::ExecutionError(error.to_string()),
                    total_usage.take(),
                )
            })?;
            combine_usage(&mut total_usage, usage);
            response.role = Role::User;
            if is_final {
                final_message = Some(response);
            } else {
                summaries.push(format_message_for_compacting(&response));
            }
        }

        if let Some(message) = final_message {
            return Ok((
                message,
                total_usage.expect("a completed compaction request records usage"),
            ));
        }

        chunks = pack_compaction_units(
            &summaries,
            max_bytes,
            request_context.max_tokens,
            request_context.token_counter,
        );
    }

    Err((
        ProviderError::ContextLengthExceeded(
            "Compaction summaries did not converge within the bounded reduction limit".to_string(),
        ),
        total_usage,
    ))
}

async fn do_compact(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    messages: &[Message],
    target_characters: usize,
) -> Result<(Message, ProviderUsage), anyhow::Error> {
    let agent_visible_messages: Vec<Message> = messages
        .iter()
        .filter(|msg| msg.is_agent_visible())
        .map(|msg| msg.agent_visible_content())
        .collect();

    let context = SummarizeContext {
        messages: "Conversation history is supplied in bounded user-message chunks.".to_string(),
        summary_target_characters: target_characters,
    };
    let system_prompt = render_template("compaction.md", &context)?;
    let token_counter = crate::token_counter::shared_token_counter()
        .await
        .map_err(|error| anyhow::anyhow!("Failed to create token counter: {error}"))?;
    let fast_model = crate::model_config::get_fast_model(provider.get_name(), model_config).await?;
    let main_limit = provider
        .get_context_limit(model_config)
        .await
        .unwrap_or_else(|_| model_config.context_limit());
    let fast_limit = provider
        .get_context_limit(&fast_model)
        .await
        .unwrap_or_else(|_| fast_model.context_limit());
    let max_tokens = (main_limit.min(fast_limit) / 3).clamp(1, COMPACTION_MAX_INPUT_TOKENS);
    let input_byte_budgets = [
        COMPACTION_MAX_INPUT_BYTES,
        COMPACTION_MAX_INPUT_BYTES / 2,
        COMPACTION_MAX_INPUT_BYTES / 4,
        COMPACTION_MIN_INPUT_BYTES,
    ];
    let removal_percentages = [0, 10, 25, 50, 100];
    let mut accumulated_usage = None;
    let request_context = CompactionRequestContext {
        provider,
        model_config,
        session_id,
        system_prompt: &system_prompt,
        max_tokens,
        token_counter: token_counter.as_ref(),
    };

    for remove_percent in removal_percentages {
        let filtered_messages = filter_tool_pairs(&agent_visible_messages, remove_percent);
        let mut units: Vec<String> = filtered_messages
            .iter()
            .map(format_message_for_compacting)
            .collect();
        if units.is_empty() {
            units.push("[No agent-visible conversation content]".to_string());
        }

        for max_bytes in input_byte_budgets {
            match reduce_compaction_units(&request_context, units.clone(), max_bytes).await {
                Ok((message, usage)) => {
                    combine_usage(&mut accumulated_usage, usage);
                    return Ok((
                        message,
                        accumulated_usage.expect("successful compaction records usage"),
                    ));
                }
                Err((ProviderError::ContextLengthExceeded(_), partial_usage)) => {
                    if let Some(usage) = partial_usage {
                        combine_usage(&mut accumulated_usage, usage);
                    }
                    continue;
                }
                Err((error, _)) => return Err(error.into()),
            }
        }
    }

    Err(anyhow::anyhow!(
        "Compaction could not fit within this provider's request limits. The original session was preserved; switch to another provider to compact it or start a new session with the essential context."
    ))
}

pub fn format_message_for_compacting(msg: &Message) -> String {
    let content_parts: Vec<String> = msg
        .content
        .iter()
        .filter_map(|content| match content {
            MessageContent::Text(text) => Some(text.text.clone()),
            MessageContent::Image(img) => Some(format!("[image: {}]", img.mime_type)),
            MessageContent::ToolRequest(req) => {
                if let Ok(call) = &req.tool_call {
                    Some(format!(
                        "tool_request({}): {}",
                        call.name,
                        serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| "<<invalid json>>".to_string())
                    ))
                } else {
                    Some("tool_request: [error]".to_string())
                }
            }
            MessageContent::ToolResponse(res) => {
                if let Ok(result) = &res.tool_result {
                    let text_items: Vec<String> = result
                        .content
                        .iter()
                        .filter_map(|content| {
                            content.as_text().map(|text_str| text_str.text.clone())
                        })
                        .collect();

                    if !text_items.is_empty() {
                        Some(format!("tool_response: {}", text_items.join("\n")))
                    } else {
                        Some("tool_response: [non-text content]".to_string())
                    }
                } else {
                    Some("tool_response: [error]".to_string())
                }
            }
            MessageContent::ToolConfirmationRequest(req) => {
                Some(format!("tool_confirmation_request: {}", req.tool_name))
            }
            MessageContent::ActionRequired(action) => match &action.data {
                ActionRequiredData::ToolConfirmation { tool_name, .. } => {
                    Some(format!("action_required(tool_confirmation): {}", tool_name))
                }
                ActionRequiredData::Elicitation { message, .. } => {
                    Some(format!("action_required(elicitation): {}", message))
                }
                ActionRequiredData::ElicitationResponse { id, .. } => {
                    Some(format!("action_required(elicitation_response): {}", id))
                }
            },
            MessageContent::FrontendToolRequest(req) => {
                if let Ok(call) = &req.tool_call {
                    Some(format!("frontend_tool_request: {}", call.name))
                } else {
                    Some("frontend_tool_request: [error]".to_string())
                }
            }
            MessageContent::Thinking(_) => None,
            MessageContent::RedactedThinking(_) => None,
            MessageContent::SystemNotification(notification) => {
                Some(format!("system_notification: {}", notification.msg))
            }
        })
        .collect();

    let role_str = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };

    if content_parts.is_empty() {
        format!("[{}]: <empty message>", role_str)
    } else {
        format!("[{}]: {}", role_str, content_parts.join("\n"))
    }
}

pub fn compute_tool_call_cutoff(context_limit: usize, compaction_threshold: f64) -> usize {
    let threshold = if compaction_threshold > 0.0 && compaction_threshold <= 1.0 {
        compaction_threshold
    } else {
        DEFAULT_COMPACTION_THRESHOLD
    };
    let effective_limit = (context_limit as f64 * threshold) as usize;
    (3 * effective_limit / 20_000).clamp(10, 500)
}

pub fn tool_ids_to_summarize(
    conversation: &Conversation,
    cutoff: usize,
    protect_last_n: usize,
) -> Vec<String> {
    let messages = conversation.messages();

    let mut tool_call_ids: Vec<String> = Vec::new();

    for msg in messages.iter() {
        if !msg.is_agent_visible() {
            continue;
        }

        for content in &msg.content {
            if let MessageContent::ToolRequest(req) = content {
                tool_call_ids.push(req.id.clone());
            }
        }
    }

    // Never summarize the last N tool calls (current turn)
    let eligible = tool_call_ids.len().saturating_sub(protect_last_n);
    if eligible <= cutoff + TOOLCALL_SUMMARIZATION_BATCH_SIZE {
        return Vec::new();
    }

    tool_call_ids
        .into_iter()
        .take(TOOLCALL_SUMMARIZATION_BATCH_SIZE)
        .collect()
}

pub async fn summarize_tool_call(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    conversation: &Conversation,
    tool_id: &str,
) -> Result<Message> {
    let messages = conversation.messages();

    let matching_messages: Vec<&Message> = messages
        .iter()
        .filter(|m| {
            m.content.iter().any(|c| match c {
                MessageContent::ToolRequest(req) => req.id == tool_id,
                MessageContent::ToolResponse(resp) => resp.id == tool_id,
                _ => false,
            })
        })
        .collect();

    if matching_messages.is_empty() {
        return Err(anyhow::anyhow!(
            "No messages found for tool id: {}",
            tool_id
        ));
    }

    let formatted = matching_messages
        .iter()
        .map(|msg| format_message_for_compacting(msg))
        .collect::<Vec<_>>()
        .join("\n");

    let user_message = Message::user().with_text(formatted);
    let summarization_request = vec![user_message];

    let system_prompt = indoc! {r#"
                Your task is to summarize a tool call & response pair to save tokens.

                Reply with a single message that describes what happened. Typically a tool call
                asks for something using a bunch of parameters and then the result is also some
                structured output. So the tool might ask to look up something on github and the
                reply might be a json document. So you could reply with something like:

                "A call to github was made to get the project status"

                if that is what it was.
            "#};

    let (mut response, _) = crate::model_config::complete_fast(
        provider,
        model_config,
        session_id,
        system_prompt,
        &summarization_request,
        &[],
    )
    .await?;

    response.role = Role::User;
    response.created = matching_messages.last().unwrap().created;
    response.metadata = MessageMetadata::agent_only();

    Ok(response.with_generated_id())
}

pub fn maybe_summarize_tool_pairs(
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    session_id: String,
    conversation: &Conversation,
    cutoff: usize,
    protect_last_n: usize,
) -> Option<JoinHandle<Vec<(Message, String)>>> {
    if !tool_pair_summarization_enabled() || provider.manages_own_context() {
        return None;
    }

    let tool_ids = tool_ids_to_summarize(conversation, cutoff, protect_last_n);
    if tool_ids.is_empty() {
        return None;
    }
    let conversation = conversation.clone();

    Some(tokio::spawn(async move {
        let mut results = Vec::new();
        for tool_id in tool_ids {
            match summarize_tool_call(
                provider.as_ref(),
                &model_config,
                &session_id,
                &conversation,
                &tool_id,
            )
            .await
            {
                Ok(summary) => results.push((summary, tool_id)),
                Err(e) => {
                    warn!("Failed to summarize tool pair: {}", e);
                }
            }
        }
        results
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use gosling_providers::conversation::token_usage::Usage;
    use rmcp::model::{AnnotateAble, CallToolRequestParams, RawContent, Tool};

    fn create_tool_pair(
        call_id: &str,
        response_id: &str,
        tool_name: &str,
        response_text: &str,
    ) -> Vec<Message> {
        vec![
            Message::assistant()
                .with_tool_request(
                    call_id,
                    Ok(CallToolRequestParams::new(tool_name.to_string())),
                )
                .with_id(call_id),
            Message::user()
                .with_tool_response(
                    call_id,
                    Ok(rmcp::model::CallToolResult::success(vec![
                        RawContent::text(response_text).no_annotation(),
                    ])),
                )
                .with_id(response_id),
        ]
    }

    struct MockProvider {
        message: Message,
        config: ModelConfig,
        max_input_bytes: Option<usize>,
        reject_context: bool,
        input_sizes: std::sync::Mutex<Vec<usize>>,
        system_sizes: std::sync::Mutex<Vec<usize>>,
        remaining_transient_failures: std::sync::atomic::AtomicUsize,
    }

    impl MockProvider {
        fn new(message: Message, context_limit: usize) -> Self {
            Self {
                message,
                config: ModelConfig {
                    model_name: "test".to_string(),
                    context_limit: Some(context_limit),
                    temperature: None,
                    max_tokens: None,
                    toolshim: false,
                    toolshim_model: None,
                    request_params: None,
                    reasoning: None,
                },
                max_input_bytes: None,
                reject_context: false,
                input_sizes: std::sync::Mutex::new(Vec::new()),
                system_sizes: std::sync::Mutex::new(Vec::new()),
                remaining_transient_failures: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn with_max_input_bytes(mut self, max: usize) -> Self {
            self.max_input_bytes = Some(max);
            self
        }

        fn rejecting_context(mut self) -> Self {
            self.reject_context = true;
            self
        }

        fn with_transient_failures(mut self, count: usize) -> Self {
            self.remaining_transient_failures = std::sync::atomic::AtomicUsize::new(count);
            self
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn get_name(&self) -> &str {
            "mock"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            system: &str,
            messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            if self
                .remaining_transient_failures
                .fetch_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |remaining| remaining.checked_sub(1),
                )
                .is_ok()
            {
                return Err(ProviderError::NetworkError(
                    "Stream decode error: error decoding response body".to_string(),
                ));
            }

            let input_bytes = messages
                .iter()
                .flat_map(|message| &message.content)
                .filter_map(MessageContent::as_text)
                .map(|text| text.len())
                .sum();
            self.input_sizes.lock().unwrap().push(input_bytes);
            self.system_sizes.lock().unwrap().push(system.len());

            if self.reject_context || self.max_input_bytes.is_some_and(|max| input_bytes > max) {
                return Err(ProviderError::ContextLengthExceeded(format!(
                    "Input too large: {input_bytes} bytes"
                )));
            }

            let message = self.message.clone();
            let usage = ProviderUsage::new("mock-model".to_string(), Usage::default());
            Ok(stream_from_single_message(message, usage))
        }

        async fn get_context_limit(
            &self,
            _model_config: &ModelConfig,
        ) -> Result<usize, ProviderError> {
            Ok(self.config.context_limit())
        }
    }

    #[tokio::test]
    async fn test_keeps_tool_request() {
        let response_message = Message::assistant().with_text("<mock summary>");
        let provider = MockProvider::new(response_message, 10_000);
        let basic_conversation = vec![
            Message::user().with_text("read hello.txt"),
            Message::assistant()
                .with_tool_request("tool_0", Ok(CallToolRequestParams::new("read_file"))),
            Message::user().with_tool_response(
                "tool_0",
                Ok(rmcp::model::CallToolResult::success(vec![
                    RawContent::text("hello, world").no_annotation(),
                ])),
            ),
        ];

        let conversation = Conversation::new_unvalidated(basic_conversation);
        let model_config = provider.config.clone();
        let (compacted_conversation, _usage) = compact_messages(
            &provider,
            &model_config,
            "test-session-id",
            &conversation,
            false,
            None,
        )
        .await
        .unwrap();

        let agent_conversation = compacted_conversation.agent_visible_messages();

        let _ = Conversation::new(agent_conversation)
            .expect("compaction should produce a valid conversation");
    }

    fn turns(count: usize) -> Vec<Message> {
        let mut messages = Vec::new();
        for i in 1..=count {
            messages.push(Message::user().with_text(format!("turn{i} request")));
            messages.push(Message::assistant().with_text(format!("turn{i} response")));
        }
        messages
    }

    #[tokio::test]
    async fn test_protects_last_n_turns_verbatim() {
        let response_message = Message::assistant().with_text("<mock summary>");
        let provider = MockProvider::new(response_message, 10_000);

        // 13 real turns; default protect_last_n_turns is 10, so turns 4-13
        // should survive untouched while turns 1-3 get folded into the summary.
        let conversation = Conversation::new_unvalidated(turns(13));
        let model_config = provider.config.clone();
        let (compacted_conversation, _usage) = compact_messages(
            &provider,
            &model_config,
            "test-session-id",
            &conversation,
            false,
            None,
        )
        .await
        .unwrap();

        let agent_visible_text: Vec<&str> = compacted_conversation
            .messages()
            .iter()
            .filter(|m| m.is_agent_visible())
            .flat_map(|m| m.content.iter())
            .filter_map(MessageContent::as_text)
            .collect();

        for protected in [4, 13] {
            assert!(
                agent_visible_text
                    .iter()
                    .any(|t| t.contains(&format!("turn{protected} request"))),
                "protected turn {protected} request should remain agent-visible verbatim: {agent_visible_text:?}"
            );
            assert!(
                agent_visible_text
                    .iter()
                    .any(|t| t.contains(&format!("turn{protected} response"))),
                "protected turn {protected} response should remain agent-visible verbatim: {agent_visible_text:?}"
            );
        }
        for summarized in [1, 2, 3] {
            assert!(
                !agent_visible_text
                    .iter()
                    .any(|t| t.contains(&format!("turn{summarized} request"))),
                "turn {summarized} should have been summarized away: {agent_visible_text:?}"
            );
        }

        // The pre-compaction history remains visible to the user even though it's
        // no longer agent-visible.
        let user_visible_text: Vec<&str> = compacted_conversation
            .messages()
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(MessageContent::as_text)
            .collect();
        assert!(user_visible_text
            .iter()
            .any(|t| t.contains("turn1 request")));
    }

    #[test]
    fn test_compaction_bands_decay_with_distance() {
        // 35 pre-cutoff turns starting at message index 0, one turn per index
        // for simplicity (a real conversation interleaves assistant replies,
        // but compaction_bands only cares about turn_starts positions).
        let turn_starts: Vec<usize> = (0..35).collect();
        let compact_end = 35;

        let bands = compaction_bands(&turn_starts, compact_end, 10);

        // Fixed-width (10-turn) blocks counting back from the cutoff; the
        // oldest block is whatever remains (5 turns here).
        assert_eq!(bands.len(), 4, "expected four blocks: {bands:?}");
        assert_eq!(bands[0].start_idx, 0);
        assert_eq!(bands[0].end_idx, 5);
        assert_eq!(bands[1].start_idx, 5);
        assert_eq!(bands[1].end_idx, 15);
        assert_eq!(bands[2].start_idx, 15);
        assert_eq!(bands[2].end_idx, 25);
        assert_eq!(bands[3].start_idx, 25);
        assert_eq!(bands[3].end_idx, 35);

        // Budget decreases linearly with distance from the cutoff.
        for pair in bands.windows(2) {
            assert!(
                pair[0].target_characters < pair[1].target_characters,
                "each older block should have a strictly smaller budget than the next: {bands:?}"
            );
        }
        assert_eq!(bands[3].target_characters, COMPACT_BAND_BASE_CHARACTERS);
        assert_eq!(
            bands[2].target_characters,
            COMPACT_BAND_BASE_CHARACTERS - COMPACT_BAND_STEP_CHARACTERS
        );
    }

    #[test]
    fn test_compaction_bands_floors_at_minimum_characters() {
        // Enough turns to produce many halvings, which should floor out at
        // COMPACT_BAND_MIN_CHARACTERS rather than reaching zero.
        let turn_starts: Vec<usize> = (0..2000).collect();
        let bands = compaction_bands(&turn_starts, 2000, 1);

        assert!(bands
            .iter()
            .all(|b| b.target_characters >= COMPACT_BAND_MIN_CHARACTERS));
        assert_eq!(
            bands.first().unwrap().target_characters,
            COMPACT_BAND_MIN_CHARACTERS
        );
    }

    #[test]
    fn test_compaction_bands_empty_when_nothing_before_cutoff() {
        let turn_starts: Vec<usize> = vec![5, 10];
        assert!(compaction_bands(&turn_starts, 5, 10).is_empty());
    }

    // budget_capped_compact_end only cares about turn_starts positions, so (as in
    // the compaction_bands tests above) one turn per message index is enough.
    #[tokio::test]
    async fn test_budget_capped_compact_end_stops_once_budget_met() {
        let token_counter = crate::token_counter::shared_token_counter().await.unwrap();
        let turn_text = "word ".repeat(200);
        let messages: Vec<Message> = (0..4)
            .map(|_| Message::user().with_text(turn_text.clone()))
            .collect();
        let turn_starts: Vec<usize> = (0..4).collect();
        let one_turn_tokens = token_counter.count_chat_tokens("", &messages[0..1], &[]);

        // A budget just over one turn's worth needs a second turn to clear it,
        // so the cutoff should land after turn index 1 (message 2) rather than
        // consuming the whole eligible region.
        let end = budget_capped_compact_end(
            &messages,
            &turn_starts,
            4,
            one_turn_tokens + 1,
            token_counter.as_ref(),
        );
        assert_eq!(
            end, 2,
            "should stop as soon as cumulative removal meets the budget, leaving newer turns untouched"
        );
    }

    #[tokio::test]
    async fn test_budget_capped_compact_end_falls_back_to_ceiling_when_budget_exceeds_region() {
        let token_counter = crate::token_counter::shared_token_counter().await.unwrap();
        let messages = vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("there"),
        ];
        let turn_starts = vec![0usize, 1];

        let end = budget_capped_compact_end(
            &messages,
            &turn_starts,
            2,
            1_000_000,
            token_counter.as_ref(),
        );
        assert_eq!(
            end, 2,
            "an unreachable budget should fall back to compacting the whole eligible region, same as a None budget"
        );
    }

    #[tokio::test]
    async fn test_compact_retries_transient_network_error() {
        // Regression test: a mid-stream network/decode failure during compaction
        // (e.g. "Stream decode error: error decoding response body") used to abort
        // the whole turn immediately with no retry. It should now be retried like
        // any other transient provider error.
        let response_message = Message::assistant().with_text("<mock summary>");
        let provider = MockProvider::new(response_message, 1000).with_transient_failures(2);
        let conversation = Conversation::new_unvalidated(vec![Message::user().with_text("hi")]);
        let model_config = provider.config.clone();

        let result = compact_messages(
            &provider,
            &model_config,
            "test-session-id",
            &conversation,
            false,
            None,
        )
        .await;

        assert!(
            result.is_ok(),
            "Compaction should recover from transient network errors via retry: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_compaction_retries_with_smaller_bounded_chunks() {
        let response_message = Message::assistant().with_text("<mock summary>");
        let provider = MockProvider::new(response_message, 258_400).with_max_input_bytes(40_000);
        let messages = vec![Message::user().with_text("x".repeat(300_000))];

        let conversation = Conversation::new_unvalidated(messages);
        let model_config = provider.config.clone();
        let result = compact_messages(
            &provider,
            &model_config,
            "test-session-id",
            &conversation,
            false,
            None,
        )
        .await;

        assert!(
            result.is_ok(),
            "Should succeed after reducing the request budget: {:?}",
            result.err()
        );
        let input_sizes = provider.input_sizes.lock().unwrap();
        assert!(input_sizes.iter().any(|size| *size > 40_000));
        assert!(input_sizes.iter().any(|size| *size <= 40_000));
        assert!(input_sizes
            .iter()
            .all(|size| *size <= COMPACTION_MAX_INPUT_BYTES));
        assert!(provider
            .system_sizes
            .lock()
            .unwrap()
            .iter()
            .all(|size| *size < 64 * 1024));
    }

    #[tokio::test]
    async fn test_failed_compaction_preserves_original_conversation() {
        let response_message = Message::assistant().with_text("<mock summary>");
        let provider = MockProvider::new(response_message, 258_400).rejecting_context();
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("important original request"),
            Message::assistant().with_text("important original response"),
        ]);
        let original = conversation.clone();
        let model_config = provider.config.clone();

        let result = compact_messages(
            &provider,
            &model_config,
            "test-session-id",
            &conversation,
            true,
            None,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(conversation.messages(), original.messages());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("original session was preserved"));
    }

    fn tool_response_count(messages: &[Message]) -> usize {
        messages
            .iter()
            .filter(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, MessageContent::ToolResponse(_)))
            })
            .count()
    }

    #[test]
    fn test_filter_tool_pairs_removes_single_pair_at_full_removal() {
        let mut messages = vec![Message::user().with_text("start")];
        messages.extend(create_tool_pair("call0", "resp0", "read_file", "content"));
        messages[1]
            .content
            .push(MessageContent::text("request context"));
        messages[2]
            .content
            .push(MessageContent::text("response context"));

        let filtered = filter_tool_pairs(&messages, 100);
        assert_eq!(tool_response_count(&filtered), 0);
        assert!(!filtered.iter().any(|message| message
            .content
            .iter()
            .any(|content| matches!(content, MessageContent::ToolRequest(_)))));
        assert!(filtered.iter().any(|message| message
            .content
            .iter()
            .any(|content| content.as_text() == Some("request context"))));
        assert!(filtered.iter().any(|message| message
            .content
            .iter()
            .any(|content| content.as_text() == Some("response context"))));
    }

    #[test]
    fn test_filter_tool_pairs_removes_all_for_odd_count() {
        let mut messages = vec![Message::user().with_text("start")];
        for i in 0..5 {
            messages.extend(create_tool_pair(
                &format!("call{}", i),
                &format!("resp{}", i),
                "read_file",
                "content",
            ));
        }

        let filtered = filter_tool_pairs(&messages, 100);
        assert_eq!(tool_response_count(&filtered), 0);
        assert!(!filtered.iter().any(|message| message
            .content
            .iter()
            .any(|content| matches!(content, MessageContent::ToolRequest(_)))));
    }

    #[test]
    fn test_filter_tool_pairs_partial_removal_is_middle_out() {
        let mut messages = Vec::new();
        for i in 0..10 {
            messages.extend(create_tool_pair(
                &format!("call{}", i),
                &format!("resp{}", i),
                "read_file",
                &format!("content{}", i),
            ));
        }

        let filtered = filter_tool_pairs(&messages, 50);
        assert_eq!(tool_response_count(&filtered), 5);

        // The first and last tool responses survive a partial removal.
        let texts: Vec<String> = filtered
            .iter()
            .filter_map(|m| m.content.iter().find_map(|c| c.as_tool_response_text()))
            .collect();
        assert!(texts.iter().any(|t| t.contains("content0")));
        assert!(texts.iter().any(|t| t.contains("content9")));
    }

    #[test]
    fn test_compute_tool_call_cutoff_scales_with_context() {
        // Default threshold (0.8)
        assert_eq!(compute_tool_call_cutoff(128_000, 0.8), 15); // 102K effective
        assert_eq!(compute_tool_call_cutoff(200_000, 0.8), 24); // 160K effective
        assert_eq!(compute_tool_call_cutoff(1_000_000, 0.8), 120); // 800K effective
                                                                   // Clamp at minimum
        assert_eq!(compute_tool_call_cutoff(50_000, 0.8), 10);
        assert_eq!(compute_tool_call_cutoff(10_000, 0.8), 10);
        // Clamp at maximum (500)
        assert_eq!(compute_tool_call_cutoff(10_000_000, 0.8), 500);
        // Lower compaction threshold means earlier summarization
        assert_eq!(compute_tool_call_cutoff(200_000, 0.3), 10); // 60K effective
        assert_eq!(compute_tool_call_cutoff(1_000_000, 0.5), 75); // 500K effective
                                                                  // Invalid threshold falls back to default 0.8
        assert_eq!(compute_tool_call_cutoff(200_000, 0.0), 24); // falls back to 0.8
        assert_eq!(compute_tool_call_cutoff(200_000, -1.0), 24); // falls back to 0.8
    }

    #[test]
    fn test_tool_ids_to_summarize_triggers_at_cutoff_plus_batch() {
        // cutoff=5, so we need >5+10=15 to trigger. 15 exactly should NOT trigger.
        let mut messages = vec![Message::user().with_text("hello")];
        for i in 0..15 {
            messages.extend(create_tool_pair(
                &format!("call{}", i),
                &format!("resp{}", i),
                "read_file",
                "content",
            ));
        }
        let conversation = Conversation::new_unvalidated(messages);
        let result = tool_ids_to_summarize(&conversation, 5, 0);
        assert!(result.is_empty(), "Exactly cutoff+batch should not trigger");

        // 16 tool calls: now exceeds cutoff+10, should return a batch of 10
        let mut messages = vec![Message::user().with_text("hello")];
        for i in 0..16 {
            messages.extend(create_tool_pair(
                &format!("call{}", i),
                &format!("resp{}", i),
                "read_file",
                "content",
            ));
        }
        let conversation = Conversation::new_unvalidated(messages);
        let result = tool_ids_to_summarize(&conversation, 5, 0);
        assert_eq!(result.len(), TOOLCALL_SUMMARIZATION_BATCH_SIZE);
        assert_eq!(result[0], "call0");
        assert_eq!(result[9], "call9");
    }

    #[test]
    fn test_tool_ids_to_summarize_protects_current_turn() {
        // 20 tool pairs, cutoff=2 → 20 > 12, would normally trigger
        let mut messages = vec![Message::user().with_text("hello")];
        for i in 0..20 {
            messages.extend(create_tool_pair(
                &format!("call{}", i),
                &format!("resp{}", i),
                "read_file",
                "content",
            ));
        }
        let conversation = Conversation::new_unvalidated(messages);

        // No protection: 20 eligible, 20 > 12 → batch of 10
        let result = tool_ids_to_summarize(&conversation, 2, 0);
        assert_eq!(result.len(), TOOLCALL_SUMMARIZATION_BATCH_SIZE);

        // Protect last 8: 12 eligible, 12 <= 12 → nothing
        let result = tool_ids_to_summarize(&conversation, 2, 8);
        assert!(
            result.is_empty(),
            "Should not summarize when protected count leaves eligible <= cutoff + batch"
        );

        // Protect last 7: 13 eligible, 13 > 12 → batch of 10
        let result = tool_ids_to_summarize(&conversation, 2, 7);
        assert_eq!(result.len(), TOOLCALL_SUMMARIZATION_BATCH_SIZE);
        assert_eq!(result[0], "call0");
    }

    // compact_messages preserves the most recent user message across the compaction
    // boundary so the agent keeps its current context. Agent-only messages (e.g. goal
    // nudges) are valid user-role messages and must be found and preserved with their
    // original visibility intact — not promoted to fully visible.
    #[tokio::test]
    async fn test_compact_messages_preserves_agent_only_user_message() {
        let response_message = Message::assistant().with_text("<mock summary>");
        let provider = MockProvider::new(response_message, 10_000);
        let model_config = provider.config.clone();

        let agent_only_msg = Message::user()
            .with_text("Focus on completing the task.")
            .with_visibility(false, true); // user_visible=false, agent_visible=true

        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("Hello"),
            Message::assistant().with_text("Hi there"),
            agent_only_msg,
        ]);

        let (compacted, _usage) = compact_messages(
            &provider,
            &model_config,
            "test-session-id",
            &conversation,
            false,
            None,
        )
        .await
        .unwrap();

        let preserved = compacted.messages().iter().find(|msg| {
            msg.is_agent_visible()
                && msg.content.iter().any(|c| {
                    if let MessageContent::Text(text) = c {
                        text.text.contains("Focus on completing")
                    } else {
                        false
                    }
                })
        });

        let preserved =
            preserved.expect("Agent-only user message should be preserved through compaction");

        assert!(
            preserved.is_agent_visible(),
            "Preserved message should be agent-visible"
        );
        assert!(
            !preserved.is_user_visible(),
            "Preserved message should remain agent-only (not user-visible) after compaction"
        );
    }

    // session.usage.total_tokens is set at the time of the LLM call, before tool
    // responses are appended. A large tool output can push the real context size
    // above the compaction threshold without updating the stored count. The check
    // must therefore use the higher of the two values to catch this case.
    #[tokio::test]
    async fn test_check_if_compaction_needed_uses_estimated_when_larger_than_stored() {
        let response_message = Message::assistant().with_text("<mock summary>");
        // context_limit=200, threshold=0.8 → triggers at 160 tokens
        let provider = MockProvider::new(response_message, 200);

        // Stored total (50) is below the 160-token threshold.
        let session = crate::session::Session {
            usage: Usage::new(Some(40), Some(10), Some(50)),
            ..crate::session::Session::default()
        };

        // Two messages of ~100 tokens each → estimated ≈ 200 > 160.
        let long_text: String = "hello world ".repeat(50);
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text(&long_text),
            Message::assistant().with_text(&long_text),
        ]);

        let needs_compact =
            check_if_compaction_needed(&provider, &conversation, Some(0.8), &session)
                .await
                .unwrap();

        assert!(
            needs_compact,
            "Compaction should be needed: estimated tokens exceed threshold \
             even though stored tokens (50) are below it (160)"
        );
    }

    // When auto-compact is disabled (threshold 0 or 1), check_if_compaction_needed
    // must return false immediately without touching the tokenizer.
    #[tokio::test]
    async fn test_check_if_compaction_needed_returns_false_when_disabled() {
        let provider = MockProvider::new(Message::assistant().with_text("x"), 1_000);
        let session = crate::session::Session::default();
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("Hello"),
            Message::assistant().with_text("Hi"),
        ]);

        for disabled_threshold in [0.0, 1.0, 1.5] {
            let result = check_if_compaction_needed(
                &provider,
                &conversation,
                Some(disabled_threshold),
                &session,
            )
            .await
            .unwrap();

            assert!(
                !result,
                "Compaction should be disabled for threshold {disabled_threshold}"
            );
        }
    }

    // reduction >= threshold (or <= 0) would put the target at or above the
    // trigger point itself, so it must disable the soft path and signal
    // callers to fall back to a full collapse instead of silently no-op'ing.
    #[tokio::test]
    async fn test_auto_compact_reduction_budget_disabled_when_misconfigured() {
        let provider = MockProvider::new(Message::assistant().with_text("x"), 1_000);
        let session = crate::session::Session::default();
        let conversation = Conversation::new_unvalidated(vec![Message::user().with_text("hi")]);

        for reduction in [0.0, -0.1, 0.6, 0.8] {
            let result = auto_compact_reduction_budget(
                &provider,
                &conversation,
                &session,
                Some(0.6),
                Some(reduction),
            )
            .await
            .unwrap();
            assert!(
                result.is_none(),
                "reduction {reduction} against threshold 0.6 should disable the soft path"
            );
        }
    }

    #[tokio::test]
    async fn test_auto_compact_reduction_budget_targets_threshold_minus_reduction() {
        // context_limit=1000, threshold=0.8, reduction=0.15 -> target = 650 tokens.
        let provider = MockProvider::new(Message::assistant().with_text("x"), 1_000);
        let session = crate::session::Session {
            usage: Usage::new(Some(900), Some(0), Some(900)),
            ..crate::session::Session::default()
        };
        let conversation = Conversation::new_unvalidated(vec![Message::user().with_text("hi")]);

        let budget = auto_compact_reduction_budget(
            &provider,
            &conversation,
            &session,
            Some(0.8),
            Some(0.15),
        )
        .await
        .unwrap();

        assert_eq!(
            budget,
            Some(250),
            "should ask to remove current (900) minus the threshold-relative target (650), \
             regardless of how the 900 was reached"
        );
    }
}
