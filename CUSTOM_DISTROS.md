# Custom Distributions of gosling

> **Tip:** This is sometimes referred to as "white labelling" — creating a branded or tailored version of an open source project for your organization.

This guide explains how to create custom distributions of gosling tailored to your organization's needs—whether that's preconfigured models, custom tools, branded interfaces, or entirely new user experiences.

## Overview

gosling's architecture is designed for extensibility. Organizations can create "remixed" versions that:

- **Preconfigure AI providers**: Ship with a specific model and non-secret credential-profile references; provision actual credentials separately
- **Bundle custom tools**: Include proprietary extensions for internal data sources
- **Customize the experience**: Modify branding, UI, and default behaviors
- **Target specific audiences**: Create specialized versions for developers, legal teams, designers, etc.

## Architecture at a Glance

```
┌─────────────────────────────────────────────────────────────────┐
│                        User Interfaces                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  CLI        │  │  Desktop    │  │  Your Custom UI         │  │
│  │  (gosling-cli)│  │  (Electron) │  │  (web, mobile, etc.)    │  │
│  └──────┬──────┘  └──────┬──────┘  └────────────┬────────────┘  │
└─────────┼────────────────┼──────────────────────┼───────────────┘
          │                │                      │
          │ (linked        │ (spawns              │ (ACP over
          │  library)      │  `gosling serve`)     │  stdio/WebSocket)
          ▼                ▼                      ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Core (gosling crate)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  Providers  │  │  Extensions │  │  Config & Defaults      │  │
│  │  (AI models)│  │  (MCP tools)│  │  (behavior & defaults)  │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

CLI and Desktop both link the `gosling` core crate directly rather than going through a separate server process; Desktop reaches it by spawning `gosling serve` as a child process and speaking ACP over WebSocket. A standalone `gosling-server` REST API crate previously existed for HTTP-based integrations but was removed as unused — see [Building a New Interface](#e-building-a-new-interface-web-mobile-etc) below for the current recommended integration path for a custom UI.

## Key Customization Points

| What You Want | Where to Look | Complexity |
|---------------|---------------|------------|
| Preconfigure a model/provider | `config.yaml`, `init-config.yaml`, environment variables | Low |
| Preconfigure Desktop workspaces | `GOSLING_WORKSPACE_TEMPLATES` in first-run configuration | Low |
| Add custom AI providers | `crates/gosling/src/providers/declarative/` | Low |
| Bundle custom MCP extensions | `config.yaml` extensions section, `ui/desktop/src/built-in-extensions.json`, `ui/desktop/src/components/settings/extensions/bundled-extensions.json` | Medium |
| Modify system prompts | `crates/gosling/src/prompts/` | Low |
| Customize desktop branding | `ui/desktop/` (icons, names, colors) | Medium |
| Build a new UI (web, mobile) | Integrate via the Agent Client Protocol (ACP) | High |
| Build complex multi-step workflows | Subagents | Medium |

## Getting Started

### 1. Fork and Clone

```bash
git clone https://github.com/YOUR_ORG/gosling.git
cd gosling
```

### 2. Choose Your Customization Strategy

- **Configuration-only**: Modify config files and environment variables (no code changes)
- **Extension-based**: Add custom MCP servers for your tools (minimal core changes)
- **Deep customization**: Modify core behavior, UI, or add new providers

### 3. Build and Distribute

See [BUILDING_LINUX.md](BUILDING_LINUX.md) and [ui/desktop/README.md](ui/desktop/README.md) for platform-specific build instructions.

## Important Considerations

### Licensing

gosling is licensed under Apache License 2.0 (ASL v2). Custom distributions must:
- Include the original license and copyright notices
- Clearly indicate any modifications made
- Not use "Gosling" trademarks in ways that imply official endorsement

For detailed guidance on ASL v2 compliance, see the [Apache License FAQ](https://www.apache.org/foundation/license-faq.html).

### Contributing Back

While you're free to maintain private forks, contributing improvements upstream benefits everyone—including your distribution. Private forks that diverge significantly become expensive to maintain and miss out on security updates and new features. Consider upstreaming generic improvements while keeping only organization-specific customizations private.

### Telemetry

gosling includes optional telemetry (via PostHog) to help improve the project. For custom distributions, you can:
- **Disable telemetry**: Set `GOSLING_DISABLE_TELEMETRY=1`
- **Use your own instance**: Modify `crates/gosling/src/posthog.rs` to point to your PostHog instance

### Staying Current

To benefit from upstream improvements:
1. Regularly sync your fork with the main repository
2. Keep customizations isolated (config files, separate extension repos) when possible
3. Subscribe to release announcements for breaking changes

---

# Appendix: Custom Distribution Scenarios

## A. Preconfigured Local Model Distribution

**Goal**: Ship gosling preconfigured to use a local Ollama model, requiring no API keys.

### Steps

1. **Create an init-config.yaml** in your distribution root:

```yaml
# init-config.yaml - Applied on first run if no config exists
GOSLING_PROVIDER: ollama
GOSLING_MODEL: qwen3-coder:latest
```

2. **Set environment defaults** in your launcher script or packaging:

```bash
export GOSLING_PROVIDER=ollama
export GOSLING_MODEL=qwen3-coder:latest
export OLLAMA_HOST=http://localhost:11434  # Or your hosted instance
```

3. **Optionally hide provider selection** in the UI by modifying `ui/desktop/src/` components.

### Technical Details

- Provider configuration: `crates/gosling/src/config/base.rs`
- Ollama provider implementation: `crates/gosling/src/providers/ollama.rs`
- Config precedence: Environment variables → config.yaml → defaults

---

## B. Corporate Distribution with Managed API Keys

**Goal**: Distribute gosling internally with pre-provisioned API keys for a frontier model.

### Steps

1. **Store API keys securely** using gosling's secret management:

```yaml
# config.yaml (distributed with your package)
GOSLING_PROVIDER: anthropic
GOSLING_MODEL: claude-sonnet-4-20250514
```

2. **Provision secrets separately** through gosling's existing secure configuration path. An
   installer or MDM integration should call the same `Config` secure setter used by Desktop; for
   manual setup, run the interactive `gosling configure` flow:

```bash
# Enter the provider secret interactively; do not place it in a workspace template.
gosling configure
```

3. **Lock down provider changes** (optional) by modifying the settings UI.

### Technical Details

- Secret storage: `crates/gosling/src/config/base.rs` (SecretStorage enum)
- Keyring integration: Uses system keyring by default, file-based fallback available
- Config file location: `~/.config/gosling/config.yaml`

---

## C. Custom Tools for Internal Data Sources

**Goal**: Add MCP extensions that connect to your data lake, internal APIs, or proprietary systems.

### Steps

1. **Create your MCP server** following the [MCP specification](https://modelcontextprotocol.io/):

```python
# Example: internal_data_mcp.py
from mcp.server import Server
from mcp.types import Tool

server = Server("internal-data")

@server.tool()
async def query_data_lake(query: str) -> str:
    """Query the corporate data lake."""
    # Your implementation here
    return results
```

2. **Bundle as a built-in extension** by adding to either:
   - `ui/desktop/src/built-in-extensions.json` (core built-ins surfaced in extension UI)
   - `ui/desktop/src/components/settings/extensions/bundled-extensions.json` (bundled extension catalog in Settings)

Example:

```json
{
  "id": "internal-data",
  "name": "Internal Data Lake",
  "description": "Query corporate data sources",
  "enabled": true,
  "type": "stdio",
  "cmd": "python",
  "args": ["/path/to/internal_data_mcp.py"],
  "env_keys": ["INTERNAL_DATA_API_KEY"],
  "timeout": 300
}
```

### Technical Details

- Extension types: `crates/gosling/src/agents/extension.rs` (ExtensionConfig enum)
- Built-in MCP servers: `crates/gosling-mcp/`
- Extension loading: `crates/gosling/src/agents/extension_manager.rs`

---

## D. Custom Branding and UI

**Goal**: Rebrand the desktop application with your organization's identity.

### Steps

1. **Replace visual assets** in `ui/desktop/src/images/`:
   - `icon.png`, `icon.ico`, `icon.icns` - Application icons
   - Update splash screens and logos as needed

2. **Modify application metadata** in `ui/desktop/forge.config.ts`:

```typescript
// forge.config.ts
module.exports = {
  packagerConfig: {
    name: 'YourCompany AI Assistant',
    executableName: 'yourcompany-ai',
    icon: 'src/images/your-icon',
    // ...
  },
  // ...
};
```

3. **Update the system prompt** to reflect your branding in `crates/gosling/src/prompts/system.md`:

```markdown
You are an AI assistant called [YourName], created by [YourCompany].
...
```

4. **Customize UI components** in `ui/desktop/src/` (React/TypeScript):
   - Color schemes in CSS/Tailwind config
   - Component text and labels
   - Feature visibility

5. **Align packaging and updater names** when rebranding:
   - Update static branding metadata in `ui/desktop/package.json` (`productName`, description) and Linux desktop templates (`ui/desktop/forge.deb.desktop`, `ui/desktop/forge.rpm.desktop`)

   - Set build/release environment variables consistently:
     - `GITHUB_OWNER` and `GITHUB_REPO` for publisher + updater repository lookup
     - `GOSLING_BUNDLE_NAME` for bundle/debug scripts and updater asset naming (defaults to `Gosling`)

Example:

```bash
export GITHUB_OWNER="your-org"
export GITHUB_REPO="your-gosling-fork"
export GOSLING_BUNDLE_NAME="InsightStream-gosling"
```

6. **Use this branding consistency checklist** before release:
   - Application metadata (`forge.config.ts`, `package.json`, `index.html`) uses your distro name
   - Release artifact names and updater lookup names are consistent
   - Desktop launchers (Linux `.desktop` templates) point to the same executable name produced by packaging

### Technical Details

- Electron config: `ui/desktop/forge.config.ts`
- UI entry point: `ui/desktop/src/renderer.tsx`
- System prompts: `crates/gosling/src/prompts/`

---

## E. Building a New Interface (Web, Mobile, etc.)

**Goal**: Create an entirely new frontend while leveraging gosling's backend.

gosling's supported integration path for building custom UIs is the Agent Client Protocol (ACP). A `gosling-server` REST API crate previously existed as an alternative but was removed as unused (nothing in this workspace built or shipped it); if a REST-based integration surface is needed again, it would need to be rebuilt rather than resurrected from history.

### Agent Client Protocol (ACP)

For richer integrations (IDEs, desktop apps, embedded agents), use the **Agent Client Protocol (ACP)**—a standardized JSON-RPC protocol for AI agent communication over stdio or other transports.

ACP provides:
- **Bidirectional communication**: Agents can request permissions, stream updates, and receive cancellations
- **Rich tool call handling**: Detailed status updates, locations, and content for each tool invocation
- **Session management**: Create, load, and resume sessions with full conversation history
- **MCP server integration**: Dynamically add MCP servers to sessions

**Start gosling as an ACP agent**:

```bash
# Run gosling as an ACP server on stdio
gosling acp --with-builtin developer

# Or programmatically
cargo run -p gosling-cli -- acp --with-builtin developer
```

**Key ACP methods**:

| Method | Description |
|--------|-------------|
| `initialize` | Establish connection and exchange capabilities |
| `session/new` | Create a new session with optional MCP servers |
| `session/load` | Resume an existing session by ID |
| `session/prompt` | Send a prompt and receive streaming responses |
| `session/cancel` | Cancel an in-progress prompt |

**Example: Python ACP client** (see `test_acp_client.py` for a complete example):

```python
import subprocess
import json

class AcpClient:
    def __init__(self):
        self.process = subprocess.Popen(
            ['gosling', 'acp', '--with-builtin', 'developer'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True
        )
    
    def send_request(self, method, params=None):
        request = {"jsonrpc": "2.0", "method": method, "id": 1}
        if params:
            request["params"] = params
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        return json.loads(self.process.stdout.readline())

# Initialize and create session
client = AcpClient()
client.send_request("initialize", {"protocolVersion": "2025-01-01"})
session = client.send_request("session/new", {"cwd": "/path/to/project"})

# Send a prompt (responses stream as notifications)
client.send_request("session/prompt", {
    "sessionId": session["result"]["sessionId"],
    "prompt": [{"type": "text", "text": "List files in this directory"}]
})
```

**ACP notifications** (sent from agent to client):
- `session/notification` with `agentMessageChunk` - Streaming text responses
- `session/notification` with `toolCall` - Tool invocation started
- `session/notification` with `toolCallUpdate` - Tool status/result updates
- `requestPermission` - Agent requests user confirmation for sensitive operations

For the full ACP specification, see the [Agent Client Protocol documentation](https://github.com/anthropics/anthropic-cookbook/tree/main/misc/agent_client_protocol).

### Technical Details

**ACP**:
- ACP server implementation: `crates/gosling/src/acp/server.rs`
- CLI integration: `crates/gosling-cli/src/cli.rs` (Command::Acp)
- Protocol library: `agent-client-protocol` crate (Rust implementation of ACP)
- SDK client implementation: `ui/sdk/src/` (generated ACP types and TypeScript client)
- Test client example: `test_acp_client.py`

---

## F. Audience-Specific Distributions (Legal, Design, etc.)

**Goal**: Create a version of gosling tailored for a specific professional audience.

### Steps

1. **Tailor the system prompt** for the audience in `crates/gosling/src/prompts/system.md`:

```markdown
You are a legal research assistant. You help lawyers and paralegals with:
- Case law research
- Document review and summarization
- Contract analysis
- Legal writing assistance

Always cite sources. Flag when you're uncertain. Never provide actual legal advice.
```

2. **Preconfigure the provider and model** via `init-config.yaml` or environment variables (see scenarios A and B).

3. **Customize the UI** to show only relevant features and use domain-appropriate language.

4. **Bundle domain-specific extensions** for specialized data sources (legal databases, design tools, etc.):

```yaml
# config.yaml
extensions:
  legal-database:
    type: stdio
    cmd: python
    args: ["/opt/legal-gosling/legal_db_mcp.py"]
    description: Legal database search
    enabled: true
```

### Technical Details

- System prompts: `crates/gosling/src/prompts/`
- Extension configuration: `crates/gosling/src/agents/extension.rs` (ExtensionConfig enum)

---

## G. Adding a Custom AI Provider

**Goal**: Add support for a new AI provider or your self-hosted model endpoint.

### Option 1: Declarative Provider (No Code)

Create a JSON file in `~/.config/gosling/custom_providers/` or bundle in your distribution:

```json
{
  "name": "my_provider",
  "engine": "openai",
  "display_name": "My Custom Provider",
  "description": "Our internal LLM endpoint",
  "api_key_env": "MY_PROVIDER_API_KEY",
  "base_url": "https://llm.internal.company.com/v1/chat/completions",
  "models": [
    {
      "name": "company-llm-v1",
      "context_limit": 32768
    }
  ],
  "supports_streaming": true,
  "requires_auth": true
}
```

Supported engines: `openai`, `anthropic`, `ollama`

### Option 2: Custom Provider (Code)

For providers with unique APIs, implement the Provider trait:

1. Create a new file in `crates/gosling/src/providers/`
2. Implement the `Provider` trait from `base.rs`
3. Register in `crates/gosling/src/providers/factory.rs`

### Technical Details

- Declarative providers: `crates/gosling/src/config/declarative_providers.rs`
- Provider trait: `crates/gosling/src/providers/base.rs`
- Provider registration: `crates/gosling/src/providers/factory.rs`
- Example providers: `crates/gosling/src/providers/declarative/*.json`

---

## H. Preconfigured Desktop Workspaces

**Goal**: Materialize repeatable, non-secret workspace definitions on first launch.

Add `GOSLING_WORKSPACE_TEMPLATES` to the layered configuration applied by your distribution. Use
stable UUIDs so an installer can provision the matching credential-profile secret through gosling's
existing `Config` secure-storage abstraction.

```yaml
GOSLING_WORKSPACE_TEMPLATES:
  schemaVersion: 1
  credentialProfiles:
    - id: "8b3e6d4e-34c9-4c0e-a9cf-9926b2aa3b2d"
      name: "Organization Anthropic"
      providerOrServiceId: "anthropic"
      authKind: "api_key"
      secretFieldNames:
        - "ANTHROPIC_API_KEY"
  workspaces:
    - id: "638f97aa-91a0-4768-b72e-5a1919e9db38"
      workspace:
        name: "Annual Meeting"
        description: "Shared working and delivery layout"
        workingFolder: "${HOME}/Projects/Annual-Meeting"
        folders:
          - id: "3d30ff53-42ad-4ea4-a1cb-87e93d7909ae"
            label: "Brand reference"
            path: "${HOME}/Projects/Branding"
            kind: "reference"
            access: "read"
        productOutputFolders:
          - id: "ec01b0ce-25e7-43de-94cf-f7b853314caf"
            label: "Deliverables"
            path: "${HOME}/Projects/Annual-Meeting/Deliverables"
            productTypes:
              - "document"
              - "presentation"
              - "image"
              - "export"
            isDefault: true
            createIfMissing: true
        credentialBindings:
          - id: "0b029eb4-e3e1-40ed-ab98-a0117d18e99d"
            label: "Organization Anthropic"
            credentialProfileId: "8b3e6d4e-34c9-4c0e-a9cf-9926b2aa3b2d"
            targetKind: "provider"
            targetId: "anthropic"
            isDefault: true
        defaultCredentialBindingId: "0b029eb4-e3e1-40ed-ab98-a0117d18e99d"
        defaultProvider: "anthropic"
        defaultModel: "claude-sonnet-4-20250514"
  activeWorkspaceTemplateId: "638f97aa-91a0-4768-b72e-5a1919e9db38"
```

Supported path placeholders are `${HOME}`, `${CONFIG_DIR}`, `${DATA_DIR}`, and `${CWD}`, optionally
followed by a relative suffix. A leading `~` is also supported. Other placeholders and parent
traversal are rejected.

The template contains field names and profile references only. It must never contain an API key,
token, password, cookie, private key, or OAuth credential. Provision the actual value separately
through an installer, MDM/bootstrap component, OAuth flow, or other existing code that calls
gosling's secure Config setter. The resulting internal secure identifier is
`workspace-credential::<profile UUID>::<declared field>`; do not call a platform keyring library or
edit the fallback file directly from new distribution code.

If a matching value was not securely provisioned, Desktop shows the template profile as missing or
needing authentication. It does not claim the profile is configured and does not fall back to a
different credential. Users can create a local secure profile in Desktop and relink the workspace.

Templates materialize once. Missing folders produce visible warnings; a missing primary folder must
be relinked before a chat can start. Workspace templates cannot currently set per-workspace
extension defaults because extension configuration is not cleanly session-scoped.

### Technical Details

- Template materialization: `crates/gosling/src/workspace/bootstrap.rs`
- Workspace persistence: `crates/gosling/src/workspace/store.rs`
- Secure profile resolution: `crates/gosling/src/workspace/credentials.rs`
- Desktop management: `ui/desktop/src/components/workspaces/`

---

## I. Complex Workflows with Subagents

**Goal**: Build sophisticated multi-step workflows that orchestrate multiple specialized tasks.

Subagents are independent AI instances that run with their own context. They're useful for:

- **Parallel execution**: Multiple tasks running simultaneously
- **Context isolation**: Preventing context window overflow
- **Specialized tasks**: Different model/settings per task

### Ad-hoc Subagents

Create subagents on-the-fly with custom instructions:

```
To complete this task:

1. Spawn a subagent to analyze the frontend code:
   subagent(instructions: "Analyze all React components in src/components/
            and list their props and state management patterns")

2. Spawn another subagent for the backend:
   subagent(instructions: "Document all API endpoints in src/api/
            including their request/response schemas")

3. Synthesize findings from both subagents into a unified report.
```

### Parallel Subagent Execution

Multiple subagent calls in the same message execute in parallel:

```
Run these analyses in parallel by making all subagent calls at once:

subagent(instructions: "Count lines of code by language")
subagent(instructions: "Find all TODO comments")
subagent(instructions: "List external dependencies")

Then combine the results into a codebase health report.
```

### Subagent Settings Override

Customize model, provider, or behavior per subagent:

```
Use a faster model for simple tasks:

subagent(
  instructions: "List all files modified in the last week",
  settings: {
    model: "gpt-4o-mini",
    max_turns: 3
  }
)

Use the full model for complex analysis:

subagent(
  instructions: "Review this code for security vulnerabilities",
  settings: {
    model: "claude-sonnet-4-20250514",
    temperature: 0.1
  }
)
```

### Extension Scoping

Limit which extensions a subagent can access:

```
Create a sandboxed subagent with only file reading capabilities:

subagent(
  instructions: "Analyze the README files in this project",
  extensions: ["developer"]  # Only developer extension, no network access
)
```

### Best Practices for Complex Workflows

1. **Parallelize independent tasks** - Multiple subagent calls in one message run concurrently
2. **Scope extensions appropriately** - Give subagents only the tools they need
3. **Use summary mode (default)** - Subagents return concise summaries; use `summary: false` only when you need full conversation history
4. **Handle failures gracefully** - Design workflows to continue even if one subagent fails

### Technical Details

- Subagent execution: `crates/gosling/src/agents/subagent_handler.rs`
