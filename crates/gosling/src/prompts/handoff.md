## Task Context
- A user is handing this conversation off to a brand-new session and needs a briefing so a fresh agent (you, next time) can pick the work back up without the original history.
- Write the briefing as the user's own message to that new session: second person ("you"), addressed to the agent that will read it next.
- Include concrete specifics — file paths, function/variable names, exact decisions, open questions — not vague summaries. The next agent has none of the original context beyond what's here.
- This text will be shown to the user before it's sent, so keep it clean and readable, not an internal scratchpad.

The conversation history is supplied separately in one bounded user message.
It may contain either a chronological history segment or summaries of earlier
segments. Preserve chronology and merge all supplied material into one
coherent briefing. Aim for no more than {{ summary_target_characters }}
characters so multiple summaries can be reduced safely.

Weight detail by recency: compress the earliest parts of the history the
most, and keep concrete specifics (the exact proposal, decision, file, or
open question) for the most recent portion, since the new session's next
message is most likely to refer back to it directly.

Compatibility note for customized templates: {{ messages }}

### Include the Following Sections, Only Where They Have Real Content:
1. **Goal** – What the user is trying to accomplish, in one or two sentences
2. **Done So Far** – Concrete progress: files touched, decisions made, what changed and why
3. **Current State** – Where things stand right now, including anything left mid-change
4. **Open Questions / Blockers** – Anything unresolved that needs a decision or answer
5. **Next Step** – *Include only if* there's a clear, specific next action to take

Omit any section that would otherwise be empty or filler. No new ideas, plans, or suggestions beyond what the conversation actually established.
