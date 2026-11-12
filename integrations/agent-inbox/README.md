# Agent inbox: hooks for coding agents (fork)

People address an agent in a Lific issue comment: they pick it under
"To:", @mention it, or simply reply on an issue the agent created, took
into progress, asked a person on, or commented on. Routes in project
settings and the issue sidebar send people's other comments to chosen
agents. The comment waits in the agent's **inbox** until the agent reads it.

An agent reads its inbox in two ways:

1. **`check_inbox`**, an MCP tool. Every client has it, and Lific's MCP
   instructions ask the agent to call it between steps. Whether it does is
   up to the model.
2. **A hook**, where the harness supports one: the harness itself asks
   Lific after tool calls, at session start and when the agent is about to
   stop, and puts new messages in front of the model. This does not depend
   on the model remembering. Use both.

The hook is `lific-inbox-hook` (POSIX sh + curl). Lific shapes the answer
for the harness (`--fmt`), so the script stays the same everywhere. Install:

```sh
curl -fsSL https://raw.githubusercontent.com/usunrise88/lific/feat/feed-and-touch-board/integrations/agent-inbox/lific-inbox-hook \
  -o ~/.local/bin/lific-inbox-hook && chmod +x ~/.local/bin/lific-inbox-hook
```

It reads `LIFIC_URL` and `LIFIC_API_KEY` (the agent's own key: the one its
MCP connection uses, so the inbox is that agent's). After tool calls it asks
at most every `LIFIC_INBOX_INTERVAL` seconds (20 by default); at start and
stop it always asks. Errors and timeouts print nothing, so the agent is
never blocked by Lific being away.

## Claude Code

`~/.claude/settings.json` (or the project's `.claude/settings.json`):

```json
{
  "env": { "LIFIC_URL": "https://tasks.example.com", "LIFIC_API_KEY": "lific_…" },
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "lific-inbox-hook --fmt claude --event start", "timeout": 10 }] }],
    "PostToolUse":  [{ "hooks": [{ "type": "command", "command": "lific-inbox-hook --fmt claude --event post",  "timeout": 10 }] }],
    "Stop":         [{ "hooks": [{ "type": "command", "command": "lific-inbox-hook --fmt claude --event stop",  "timeout": 10 }] }]
  }
}
```

After a tool call new messages reach the model on its next step. At Stop
the hook keeps the agent going (`decision: block`) only when something new
arrived; it is marked read on the way, so the next Stop lets it finish.

## Codex CLI

`~/.codex/hooks.json`, then approve it once with `/hooks` (Codex trusts a
hook by its hash). Put `LIFIC_URL` and `LIFIC_API_KEY` in your shell profile.

```json
{ "hooks": {
  "SessionStart": [{ "hooks": [{ "type": "command", "command": "lific-inbox-hook --fmt codex --event start", "timeout": 10 }] }],
  "PostToolUse":  [{ "hooks": [{ "type": "command", "command": "lific-inbox-hook --fmt codex --event post",  "timeout": 10 }] }],
  "Stop":         [{ "hooks": [{ "type": "command", "command": "lific-inbox-hook --fmt codex --event stop",  "timeout": 10 }] }]
} }
```

## Gemini CLI

`~/.gemini/settings.json`, key `hooks`: `SessionStart` → `--fmt gemini --event start`,
`AfterTool` → `--event post`, `AfterAgent` → `--event stop` (Gemini continues on `deny`).

## Copilot CLI, Factory Droid

Same JSON as Claude Code: Copilot CLI in `~/.copilot/hooks/lific.json`
(`sessionStart`, `postToolUse`, `agentStop`, `"bash": "lific-inbox-hook --fmt copilot --event …"`),
Droid in `~/.factory/hooks.json` with the Claude Code event names and `--fmt droid`.

## Cursor

`.cursor/hooks.json`: `{"version":1,"hooks":{"sessionStart":[{"command":"lific-inbox-hook --fmt cursor --event start"}],"postToolUse":[{"command":"lific-inbox-hook --fmt cursor --event post"}],"stop":[{"command":"lific-inbox-hook --fmt cursor --event stop"}]}}`.
In `cursor-agent` (CLI) hook support has been partial; `check_inbox` covers it.

## DeepSeek Harness (`dsh`)

Its Claude Code bridge runs the Claude Code hooks above unchanged:
`- name: '@deepseek-ai/dsh-hooks-claude-code'` with `configPath: ./.claude/settings.json` in `cordis.yml`.

## OpenCode, Kilo CLI

`opencode-lific-inbox.ts` here: copy it to `.opencode/plugin/` (Kilo:
`.kilo/plugin/`, import from `@kilocode/plugin`).

## Everything else

The instance's agent manifest already tells agents to do this (step 4,
"Listen"). Install it in the repository with `/mcp__lific__manifest` in
Claude Code, or ask any agent to "call get_manifest and put the block into
AGENTS.md". An admin edits the manifest in Instance settings. Without the
manifest, add this to `AGENTS.md` (or `CLAUDE.md`, `GEMINI.md`, `.clinerules`):

```md
People can address you in Lific issue comments. Call the Lific MCP tool
`check_inbox` between steps and whenever you finish a task. For each
message decide whether it outranks your current work, and acknowledge it
on the issue with `add_comment` (what you will do, and when).
```

Aider has no MCP, and Roo Code is archived; neither can read the inbox.
