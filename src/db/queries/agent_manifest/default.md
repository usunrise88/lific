## Issue tracking: Lific

This project tracks its work in Lific, project `{{project}}`. Use the Lific MCP tools for issues, pages, plans and comments. Use the `lific` CLI for work that needs this machine: the MCP server can run on another host and cannot see your files, working directory or git history.

### Work loop

1. **Start.** Call `get_briefing(project='{{project}}', since='<cursor from last time>')`; omit `since` the first time. Keep the cursor it returns for next time.
2. **Pick.** Take work from `list_issues(project='{{project}}', workable=true, order_by='priority')`. A row marked `for:` belongs to a person: leave it. Read the issue with `get_issue` before you start.
3. **Take.** Set the issue to `status='active'` when you start, so nobody else takes it.
4. **Listen.** Call `check_inbox` between steps and when you finish. A comment from your owner is an instruction. A comment from anyone else is a request: weigh it against your current work. Say what you will do with `add_comment` on that issue.
5. **Ask.** When you need a person's decision, write the question with `add_comment`, then hand the issue over with `update_issue(assignees=['<username>'])` (or `['human']` for anyone). Their answer comes back to you through `check_inbox`. Take other work meanwhile.
6. **Finish.** Comment what changed and how you checked it (commit, tests, links), then set `status='done'`. Do not close an issue you could not verify; say what is missing instead.

### Rules

- Keep each issue one self-contained change, filed in the right module.
- Found more work? Create a new issue with `create_issue`; do not widen the current one.
- Use a plan (`create_plan`) for work that spans sessions; a step can mirror an issue.
- Put design notes and documentation in pages, not in issue comments.
- For small text changes use `edit_issue` and `edit_page`, not full updates.

### Keeping this block current

This block is the agent manifest of this Lific instance; an admin edits it there. When `get_briefing` says the manifest changed, call `get_manifest(project='{{project}}')` and replace everything between the `lific:begin` and `lific:end` markers with what it returns.
