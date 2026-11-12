// Lific agent inbox for OpenCode (plugin API v1) and Kilo CLI (same API,
// import from "@kilocode/plugin"). Put it in .opencode/plugin/ (or
// ~/.config/opencode/plugin/) and set LIFIC_URL and LIFIC_API_KEY.
//
// After a tool runs it appends new messages to the tool's output, so the
// model reads them on its next step; when the session goes idle it sends
// them as a new prompt. Checked against the v1 docs, not yet run against
// every OpenCode release: if a hook name moved, the plugin does nothing.
import type { Plugin } from "@opencode-ai/plugin";

const URL = (process.env.LIFIC_URL ?? "").replace(/\/$/, "");
const KEY = process.env.LIFIC_API_KEY ?? "";
const INTERVAL_MS = Number(process.env.LIFIC_INBOX_INTERVAL ?? 20) * 1000;
let last = 0;

async function inbox(force: boolean): Promise<string> {
  if (!URL || !KEY) return "";
  const now = Date.now();
  if (!force && now - last < INTERVAL_MS) return "";
  last = now;
  try {
    const res = await fetch(`${URL}/api/agent/inbox/hook?fmt=text`, {
      headers: { Authorization: `Bearer ${KEY}` },
      signal: AbortSignal.timeout(5000),
    });
    return res.status === 200 ? await res.text() : "";
  } catch {
    return "";
  }
}

export const LificInbox: Plugin = async ({ client }) => ({
  "tool.execute.after": async (_input, output) => {
    const text = await inbox(false);
    if (text) output.output = `${output.output ?? ""}\n\n[Lific inbox]\n${text}`;
  },
  event: async ({ event }) => {
    if (event.type !== "session.idle") return;
    const text = await inbox(true);
    if (!text) return;
    await client.session.prompt({
      path: { id: event.properties.sessionID },
      body: { parts: [{ type: "text", text }] },
    });
  },
});
