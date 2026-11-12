<script lang="ts">
  import {
    me,
    listBots,
    createBot,
    disconnectBot,
    deleteBot,
    updateProfile,
    changePassword,
    revokeAllSessions,
    logout,
    clearSession,
    getInstance,
    TOOL_TEMPLATES,
    type Bot,
    type ToolTemplate,
  } from "../lib/api";
  import {
    RECENT_AUTH_ERROR,
    needsReauth,
    reauthenticateWithoutPassword,
    reauthenticateWithPassword,
    retryOnceAfterReauth,
  } from "../lib/reauth";
  import ToolIcon from "../lib/ToolIcon.svelte";
  import TimeAgo from "../lib/TimeAgo.svelte";
  import SettingsTabs from "../lib/SettingsTabs.svelte";
  import Skeleton from "../lib/Skeleton.svelte";
  import {
    themePreference, setPreference, type ThemePreference,
    getAccent, setAccent, type AccentPreset,
    getThemePreset, setThemePreset, CLASSIC_THEME,
    getDensity, setDensity, type Density,
    getFontScale, setFontScale, type FontScale,
    getMotionPreference, setMotionPreference, type MotionPreference,
  } from "../lib/theme";
  import { accentPresets } from "../lib/appearance/presets";
  import LanguagePicker from "../lib/LanguagePicker.svelte";
  import { t } from "../lib/i18n";
  import { RADIX_THEMES, type ThemeSwatch } from "../lib/appearance/themes";
  import {
    Plug, Check, Copy, X, AlertTriangle, Sun, Moon, Monitor,
    Palette, Lock, LogOut, Eye, EyeOff, KeyRound, FileCode2, Terminal,
    Rows3, Rows2, Type, Zap, ZapOff,
  } from "lucide-svelte";
  import { getContext } from "svelte";
  import { copyToClipboard } from "../lib/clipboard";
  import { currentUser, getUserRevision, publishUser } from "../lib/userState";

  let { navigate }: { navigate: (path: string) => void } = $props();

  const topbarCtx = getContext<{
    set: (s: import("svelte").Snippet | undefined) => void;
  } | undefined>("lific:topbar");
  $effect(() => {
    topbarCtx?.set(topbarContent);
    return () => topbarCtx?.set(undefined);
  });

  let user = $derived($currentUser);
  let bots = $state<Bot[]>([]);
  let loading = $state(true);
  let customOpen = $state(false);
  let customId = $state("");
  let customName = $state("");
  let customTemplate = $state("");
  let toolsError = $state("");

  function connectionId(bot: Bot): string {
    // Older bots can lack tool_id until their next connection backfills it.
    return bot.tool_id ?? TOOL_TEMPLATES.find((t) => bot.username.startsWith(`${t.id}-`))?.id ?? bot.username;
  }

  function genericTemplate(id: string, name: string): ToolTemplate {
    return {
      id, name, description: t("settings.custom.description"),
      configPath: { linux: null, mac: null, windows: null },
      configNote: [{ text: t("settings.custom.configNote") }],
      generateConfig: (url, key) => JSON.stringify({ url, headers: { Authorization: `Bearer ${key}` } }, null, 2),
    };
  }

  let connectionTemplates = $derived([
    ...TOOL_TEMPLATES,
    ...bots.filter((b) => b.parent_id == null && !TOOL_TEMPLATES.some((t) => t.id === connectionId(b)))
      .map((b) => genericTemplate(connectionId(b), b.display_name)),
  ]);

  function connectCustom() {
    const id = customId.trim().toLowerCase();
    const name = customName.trim() || id;
    toolsError = "";
    if (!/^[a-z0-9][a-z0-9_-]{0,47}$/.test(id)) {
      toolsError = t("settings.custom.invalidId");
      return;
    }
    if (getToolBot(id)?.connected) {
      toolsError = t("settings.custom.exists");
      return;
    }
    const template = TOOL_TEMPLATES.find((t) => t.id === customTemplate) ?? genericTemplate(id, name);
    customOpen = false;
    openConnect({ ...template, id, name });
  }

  // Connect modal
  let connectTool = $state<ToolTemplate | null>(null);
  let connectKey = $state<string | null>(null);
  let connecting = $state(false);
  let connectError = $state("");
  let copied = $state(false);
  let keyCopied = $state(false);
  let exportCopied = $state(false);
  // Which note-step command chip was just copied (by index), so only that
  // chip flashes "copied".
  let noteCopiedIdx = $state<number | null>(null);
  // The minted key is sensitive — hidden by default, revealed on demand.
  let keyRevealed = $state(false);

  // Masked rendering: keep the non-secret prefix (e.g. "lific_sk-live-")
  // legible so the user can sanity-check the key shape, dot out the rest.
  let maskedKey = $derived.by(() => {
    if (!connectKey) return "";
    const m = connectKey.match(/^(lific_sk-live-|lific_sk-test-|lific_sk-|sk-live-|sk-)/);
    const prefix = m ? m[0] : connectKey.slice(0, 6);
    const hiddenLen = Math.max(connectKey.length - prefix.length, 0);
    return prefix + "•".repeat(Math.min(hiddenLen, 40));
  });
  let displayKey = $derived(keyRevealed ? (connectKey ?? "") : maskedKey);

  // OS selector for the config-path display. Best-effort default from the
  // browser; the user can override since they may be configuring a tool on a
  // different machine than the one viewing this page.
  type Os = "linux" | "mac" | "windows";
  const OS_LABELS: Record<Os, string> = { linux: "Linux", mac: "macOS", windows: "Windows" };
  function detectOs(): Os {
    const p = `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
    if (p.includes("win")) return "windows";
    if (p.includes("mac")) return "mac";
    return "linux";
  }
  let selectedOs = $state<Os>(detectOs());

  // Group OSes that share an identical config path so the selector merges
  // them into one button (e.g. "Linux / macOS" when only Windows differs).
  // Order is stable: Linux, macOS, Windows.
  type OsGroup = { oses: Os[]; label: string };
  let osGroups = $derived.by<OsGroup[]>(() => {
    if (!connectTool) return [];
    const order: Os[] = ["linux", "mac", "windows"];
    const groups: OsGroup[] = [];
    for (const os of order) {
      const path = connectTool.configPath[os];
      // Skip OSes the tool doesn't support (null path) — e.g. Claude
      // Desktop on Linux, which Anthropic doesn't ship.
      if (!path) continue;
      const existing = groups.find(
        (g) => connectTool!.configPath[g.oses[0]] === path,
      );
      if (existing) existing.oses.push(os);
      else groups.push({ oses: [os], label: "" });
    }
    return groups.map((g) => ({
      oses: g.oses,
      label: g.oses.map((o) => OS_LABELS[o]).join(" / "),
    }));
  });
  // True when the tool has more than one distinct path across OSes.
  let pathsDiffer = $derived(osGroups.length > 1);
  let activePath = $derived(connectTool ? connectTool.configPath[selectedOs] : "");

  // The command that sets the env var. macOS and Linux share the same POSIX
  // `export` syntax (both use POSIX shells; the only difference is which
  // rc-file persists it). Windows uses `setx`, which differs meaningfully:
  // it writes to the registry for FUTURE shells and does NOT affect the
  // current one.
  let exportLine = $derived.by(() => {
    if (!connectTool?.usesEnvKey || !connectKey) return "";
    const v = connectTool.envVar ?? "LIFIC_API_KEY";
    return selectedOs === "windows"
      ? `setx ${v} "${connectKey}"`
      : `export ${v}="${connectKey}"`;
  });

  // Where to put the line so it survives new terminals (OS/shell-specific).
  let persistHint = $derived.by(() => {
    switch (selectedOs) {
      case "mac":
        return t("settings.connect.persist.mac");
      case "linux":
        return t("settings.connect.persist.linux");
      case "windows":
        return t("settings.connect.persist.windows");
    }
  });

  let busyId = $state<number | null>(null);

  // Profile edit (a proper labeled form, separate from the read-only
  // identity header — name/email aren't casual click-to-edit fields).
  let profileName = $state("");
  let profileEmail = $state("");
  let profileSaving = $state(false);
  let profileError = $state("");
  let profileSavedAt = $state(0);

  // Appearance
  let accentPref = $state<AccentPreset>("indigo");
  let themePresetPref = $state<string>(CLASSIC_THEME);

  // Fork: the original palette as the first theme, then the Radix Colors
  // ones. Classic's swatches mirror app.css (its default indigo accent).
  const THEME_CHOICES: { id: string; label: string; light: ThemeSwatch; dark: ThemeSwatch }[] = [
    {
      id: CLASSIC_THEME,
      label: t("settings.appearance.classic"),
      light: { bg: "#eef4f1", surface: "#f7faf9", accent: "#5746a0", text: "#18201d" },
      dark: { bg: "#0d1110", surface: "#252c29", accent: "#9287d7", text: "#e3e8e6" },
    },
    ...RADIX_THEMES,
  ];

  function pickThemePreset(id: string) {
    themePresetPref = id;
    setThemePreset(id);
  }
  let densityPref = $state<Density>("comfortable");
  let fontScalePref = $state<FontScale>("md");
  let motionPref = $state<MotionPreference>("system");

  // Security
  let curPw = $state("");
  let newPw = $state("");
  let pwError = $state("");
  let pwSaving = $state(false);
  let pwSuccess = $state(false);
  let signingOut = $state(false);
  // Two-step inline confirm for the destructive sign-out. It revokes every
  // credential the account owns, not just this browser's session, so it does
  // not belong behind a single click.
  let confirmingSignOutAll = $state(false);
  let signOutAllError = $state("");

  $effect(() => {
    accentPref = getAccent();
    themePresetPref = getThemePreset();
    densityPref = getDensity();
    fontScalePref = getFontScale();
    motionPref = getMotionPreference();
    loadUser();
  });

  let hasProfileChanges = $derived(
    user != null && (
      profileName.trim() !== user.display_name ||
      profileEmail.trim().toLowerCase() !== user.email
    ),
  );

  async function saveProfile() {
    if (!user || !hasProfileChanges || profileSaving) return;
    const revision = getUserRevision();
    const session = localStorage.getItem("lific_token");
    profileSaving = true;
    profileError = "";
    const input: { display_name?: string; email?: string } = {};
    if (profileName.trim() !== user.display_name) input.display_name = profileName.trim();
    if (profileEmail.trim().toLowerCase() !== user.email) input.email = profileEmail.trim();
    const res = await updateProfile(input);
    profileSaving = false;
    if (session !== localStorage.getItem("lific_token")) return;
    if (res.ok) {
      if (!publishUser(res.data, revision)) return;
      profileName = res.data.display_name;
      profileEmail = res.data.email;
      profileSavedAt = Date.now();
      window.setTimeout(() => { if (Date.now() - profileSavedAt >= 1900) profileSavedAt = 0; }, 2000);
    } else {
      profileError = res.error;
    }
  }

  function pickTheme(p: ThemePreference) {
    setPreference(p);
  }

  function pickAccent(p: AccentPreset) {
    accentPref = p;
    setAccent(p);
  }

  function pickDensity(p: Density) {
    densityPref = p;
    setDensity(p);
  }

  function pickFontScale(p: FontScale) {
    fontScalePref = p;
    setFontScale(p);
  }

  function pickMotion(p: MotionPreference) {
    motionPref = p;
    setMotionPreference(p);
  }

  async function submitPassword() {
    if (pwSaving) return;
    pwError = ""; pwSuccess = false;
    if (newPw.length < 8) { pwError = t("settings.password.tooShort"); return; }
    pwSaving = true;
    // `changePassword` adopts the replacement session token the server
    // returns. Everything after this point depends on that: the old token is
    // dead the moment the request succeeds.
    const res = await changePassword({ current_password: curPw, new_password: newPw });
    pwSaving = false;
    if (res.ok) {
      pwSuccess = true;
      curPw = ""; newPw = "";
      // Connected tools were revoked along with everything else, so re-read
      // them rather than leaving the page claiming they are still connected.
      await loadBots();
      window.setTimeout(() => { pwSuccess = false; }, 6000);
    } else {
      pwError = res.error;
    }
  }

  async function signOutAll() {
    if (signingOut) return;
    signingOut = true;
    signOutAllError = "";
    const res = await revokeAllSessions();
    signingOut = false;
    if (res.ok) {
      confirmingSignOutAll = false;
      currentUser.set(null);
      navigate("/login");
      return;
    }
    // The local token is still valid (the helper only clears it on success),
    // so stay put, keep the confirm open, and let them try again.
    signOutAllError = res.error;
  }

  async function logoutNow() {
    await logout();
    clearSession();
    currentUser.set(null);
    navigate("/login");
  }

  async function loadUser() {
    const revision = getUserRevision();
    const session = localStorage.getItem("lific_token");
    // Whether this instance signs in without a password decides how a stale
    // session is recovered below, so it is read once with the profile.
    const instance = await getInstance();
    if (instance.ok) webAutoLogin = instance.data.web_auto_login;
    const result = await me();
    if (session !== localStorage.getItem("lific_token")) return;
    if (result.ok) {
      publishUser(result.data, revision);
      profileName = user?.display_name ?? "";
      profileEmail = user?.email ?? "";
      await loadBots();
    }
    loading = false;
  }
  async function loadBots() {
    const result = await listBots();
    if (result.ok) bots = result.data;
  }

  function getToolBot(toolId: string): Bot | undefined {
    if (!user) return undefined;
    return bots.find((b) => b.parent_id == null && connectionId(b) === toolId);
  }
  /** Fork: a tool's workspace agents, one per place it runs. */
  function placesOf(bot: Bot | undefined): Bot[] {
    if (!bot) return [];
    return bots
      .filter((b) => b.parent_id === bot.id)
      .sort((a, b) => (b.last_seen_at ?? "").localeCompare(a.last_seen_at ?? ""));
  }
  function toolState(toolId: string): "connected" | "disconnected" | "none" {
    const bot = getToolBot(toolId);
    if (!bot) return "none";
    return bot.connected ? "connected" : "disconnected";
  }

  // ── Re-authentication for connecting a tool ──────────────
  //
  // Connecting a tool mints a credential that outlives the browser session, so
  // the server requires a sign-in from the last 15 minutes. A tab left open
  // over lunch hits that, and "recent authentication required" on its own is a
  // dead end: nothing on screen says what to do about it.
  //
  // Passwordless instances have no password to ask for, so we mint a fresh
  // session and retry without bothering anyone. Everywhere else the modal
  // grows one password field, and the retry happens exactly once.
  // Snapshot of which tool was being connected, and who was connecting it.
  // `connectTool` is cleared by closing the modal and `user` can be replaced
  // by a reload, so the retry must not read either back.
  let reauthNeeded = $state<{ toolId: string; userId: number } | null>(null);
  let reauthPassword = $state("");
  let reauthBusy = $state(false);
  let webAutoLogin = $state(false);
  /** Why the automatic sign-in did not work, when one was attempted. */
  let autoReauthNote = $state("");

  /** Mint the bot, and report whether staleness is what stopped us.
   *
   *  `recover` is the automatic route out of a staleness refusal, used only
   *  where one exists: a passwordless instance can mint a fresh session on its
   *  own. Everywhere else it declines, which surfaces as "stale" and puts the
   *  password prompt on screen. */
  async function attemptConnect(
    template: ToolTemplate,
    recover: boolean,
  ): Promise<"ok" | "stale" | "failed"> {
    const res = await retryOnceAfterReauth(
      () => createBot(template.id, template.name),
      async () => {
        if (!recover || !webAutoLogin || !user) {
          // `recover` false means the password prompt already refreshed the
          // session, so there is nothing left to fall back to.
          return { ok: false, error: RECENT_AUTH_ERROR, recoverable: recover };
        }
        const auto = await reauthenticateWithoutPassword(user.id);
        // A failed or wrong-account auto-login is not the end of the road:
        // note why, and fall through to the password prompt. The stored token
        // is untouched either way.
        if (!auto.ok) autoReauthNote = auto.error;
        return auto;
      },
    );
    if (res.ok) {
      connectKey = res.data.key;
      await loadBots();
      return "ok";
    }
    if (needsReauth(res)) return "stale";
    connectError = res.error;
    return "failed";
  }

  // Open the modal and mint credentials in one step (no extra confirm).
  async function openConnect(template: ToolTemplate) {
    connectTool = template;
    connectKey = null;
    connectError = "";
    copied = false;
    keyCopied = false;
    exportCopied = false;
    noteCopiedIdx = null;
    keyRevealed = false;
    reauthNeeded = null;
    reauthPassword = "";
    autoReauthNote = "";
    // Default to the viewer's OS, but if the tool has no config for it
    // (e.g. Claude Desktop on Linux), fall back to the first OS it does
    // support so the modal never opens on an empty path.
    {
      const detected = detectOs();
      selectedOs = template.configPath[detected]
        ? detected
        : ((["linux", "mac", "windows"] as Os[]).find(
            (o) => template.configPath[o],
          ) ?? "mac");
    }
    connecting = true;
    if ((await attemptConnect(template, true)) === "stale" && user) {
      reauthNeeded = { toolId: template.id, userId: user.id };
    }
    connecting = false;
  }

  /** The password path: verify, then retry the connect exactly once. */
  async function submitReauthAndConnect() {
    const pending = reauthNeeded;
    if (reauthBusy || !pending || !user || !reauthPassword) return;
    if (pending.userId !== user.id || connectTool?.id !== pending.toolId) {
      // The signed-in account or the open tool changed under the prompt.
      // Retrying would connect something nobody asked for.
      connectError = t("settings.connect.interrupted");
      reauthNeeded = null;
      reauthPassword = "";
      return;
    }
    reauthBusy = true;
    connectError = "";
    const refreshed = await reauthenticateWithPassword(reauthPassword, pending.userId);
    // Clear the field whatever happens, so a wrong password is retyped rather
    // than resubmitted by a stray Enter.
    reauthPassword = "";
    if (!refreshed.ok) {
      connectError = refreshed.error;
      reauthBusy = false;
      return;
    }
    autoReauthNote = "";
    // The session is fresh now, so this is the one retry. No recovery route
    // is offered to it: a refusal here is a real failure, not staleness.
    const outcome = await attemptConnect(connectTool, false);
    if (outcome === "ok") reauthNeeded = null;
    else if (outcome === "stale" && !connectError) {
      connectError = t("settings.connect.stillRejected");
    }
    reauthBusy = false;
  }

  function closeConnect() {
    if (connecting || reauthBusy) return;
    connectTool = null;
    connectKey = null;
    connectError = "";
    connecting = false;
    reauthNeeded = null;
    reauthPassword = "";
    reauthBusy = false;
    autoReauthNote = "";
  }

  function configText(): string {
    if (!connectTool || !connectKey) return "";
    return connectTool.generateConfig(window.location.origin + "/mcp", connectKey);
  }
  // What's rendered in the config block. Masks the embedded key to match
  // Step 1's hide-by-default behavior; copy always uses the real configText().
  let displayConfig = $derived.by(() => {
    const text = configText();
    if (keyRevealed || !connectKey) return text;
    return text.split(connectKey).join(maskedKey);
  });
  // Each copy button keeps its own inline "Copied" flip, so the shared helper
  // runs with silentSuccess (no success toast) but still fires the error toast
  // on failure — the old catch swallowed blocked-clipboard failures silently.
  async function copyWithFlag(text: string, flag: (v: boolean) => void) {
    const ok = await copyToClipboard(text, { silentSuccess: true });
    if (ok) {
      flag(true);
      window.setTimeout(() => flag(false), 1500);
    }
  }
  const copyConfig = () => copyWithFlag(configText(), (v) => (copied = v));
  const copyKey = () => connectKey && copyWithFlag(connectKey, (v) => (keyCopied = v));
  const copyExport = () => copyWithFlag(exportLine, (v) => (exportCopied = v));
  async function copyNote(idx: number, command: string) {
    await copyWithFlag(command, (v) => (noteCopiedIdx = v ? idx : null));
  }

  async function handleDisconnect(id: number) {
    busyId = id;
    toolsError = "";
    const result = await disconnectBot(id);
    if (!result.ok) toolsError = result.error;
    await loadBots();
    busyId = null;
  }
  async function handleRemove(id: number) {
    busyId = id;
    toolsError = "";
    const result = await deleteBot(id);
    if (!result.ok) toolsError = result.error;
    await loadBots();
    busyId = null;
  }

  function initials(name: string): string {
    return name.split(/[\s_-]+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? "").join("");
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === "Escape" && connectTool) closeConnect(); }} />

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-6 py-2 w-full">
    <span class="text-body-sm font-medium text-[var(--text)]">{t("settings.title")}</span>
  </div>
{/snippet}

<div class="flex-1 overflow-y-auto">
  <div class="w-full max-w-[1000px] mx-auto px-6 py-10 md:py-12">
    {#if loading}
      <!-- LIF-281: structural skeleton replacing the bare centered spinner.
           Mirrors the loaded account page's default-visible frame inside the
           same max-w-[1000px] wrapper — the tab bar (border-b + mb-8), the
           identity hero (size-14 avatar + name/meta, mb-8), and the
           Appearance card (rounded-xl surface + p-5 with its header +
           first control row) — so the first heading lands at the same
           y-position and the layout doesn't snap when data arrives. -->
      <!-- Tab bar stand-in (SettingsTabs: border-b + mb-8) -->
      <div class="flex items-center gap-6 border-b border-[var(--border)] mb-8">
        <Skeleton variant="bar" class="h-4 w-16 mb-2.5 mt-1" />
        <Skeleton variant="bar" class="h-4 w-16 mb-2.5 mt-1" />
      </div>

      <!-- Identity hero -->
      <section class="flex items-center gap-4 mb-8">
        <Skeleton variant="circle" class="size-14 shrink-0" />
        <div class="min-w-0 flex flex-col gap-2">
          <Skeleton variant="bar" class="h-6 w-48" />
          <Skeleton variant="bar" class="h-3.5 w-64" />
        </div>
      </section>

      <!-- Appearance card -->
      <section class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-5">
        <div class="flex items-center gap-2 mb-1">
          <Skeleton variant="circle" class="size-[15px] rounded" />
          <Skeleton variant="bar" class="h-4 w-32" />
        </div>
        <Skeleton variant="bar" class="h-3 w-40 mb-3.5" />
        <Skeleton variant="block" class="h-9 w-[280px] rounded-lg" />
      </section>
    {:else if user}
      <SettingsTabs active="account" isAdmin={user.is_admin} {navigate} />

      <!-- ── IDENTITY HERO (read-only) ────────────────── -->
      <section class="flex items-center gap-4 mb-8 animate-reveal delay-100">
        <div class="size-14 shrink-0 rounded-full bg-[var(--accent)] text-[var(--accent-text)] grid place-items-center font-display text-title tracking-tight">
          {initials(user.display_name || user.username)}
        </div>
        <div class="min-w-0">
          <h1 class="font-display text-title tracking-tight text-[var(--text)] leading-none truncate">
            {user.display_name || user.username}
          </h1>
          <div class="flex items-center gap-2 mt-1.5 flex-wrap text-body-sm">
            <span class="font-mono text-[var(--text-muted)]">@{user.username}</span>
            <span class="text-[var(--text-faint)]">·</span>
            <span class="text-[var(--text-muted)]">{user.email}</span>
            <span class="text-micro font-semibold uppercase tracking-wide px-1.5 py-0.5 rounded-full
                         {user.is_admin ? 'text-[var(--accent)] bg-[var(--accent-subtle)]' : 'text-[var(--text-muted)] bg-[var(--bg-subtle)]'}">
              {user.is_admin ? t("settings.role.admin") : t("settings.role.member")}
            </span>
          </div>
        </div>
      </section>

      <!-- ── APPEARANCE ───────────────────────────────── -->
      <section class="rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)] p-5 animate-reveal delay-250">
        <div class="flex items-center gap-2 mb-1">
          <Palette size={15} class="text-[var(--text-muted)]" />
          <h2 class="text-body-lg font-semibold text-[var(--text)]">{t("settings.appearance.title")}</h2>
        </div>
        <p class="text-body-sm text-[var(--text-muted)] mb-3.5">{t("settings.appearance.hint")}</p>
        <div class="inline-flex p-0.5 rounded-lg bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]">
          {#each [["light", t("settings.appearance.light"), Sun], ["dark", t("settings.appearance.dark"), Moon], ["system", t("settings.appearance.system"), Monitor]] as [val, label, Icon]}
            {@const IconComp = Icon as typeof Sun}
            <button
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-body-sm font-medium transition-all
                     {$themePreference === val
                ? 'bg-[var(--surface)] text-[var(--text)] shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
                : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
              onclick={() => pickTheme(val as ThemePreference)}
            >
              <IconComp size={14} />
              {label}
            </button>
          {/each}
        </div>

        <!-- Language (fork) -->
        <div class="mt-5 pt-5 border-t border-[var(--border)]">
          <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2.5">{t("common.language")}</span>
          <LanguagePicker />
        </div>

        <!-- Theme (fork): Classic or a Radix Colors palette. Each card previews
             the theme in light and dark. -->
        <div class="mt-5 pt-5 border-t border-[var(--border)]">
          <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2.5">{t("settings.appearance.theme")}</span>
          <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-2.5">
            {#each THEME_CHOICES as th (th.id)}
              {@const on = themePresetPref === th.id}
              <button
                type="button"
                class="group text-left rounded-xl p-1.5 transition-all motion-safe:active:scale-[0.98]
                       {on ? 'ring-2 ring-[var(--accent)] bg-[var(--accent-subtle)]' : 'ring-1 ring-[var(--border)] hover:ring-[var(--text-faint)]'}"
                aria-pressed={on}
                onclick={() => pickThemePreset(th.id)}
              >
                <span class="flex h-12 rounded-lg overflow-hidden" aria-hidden="true">
                  {#each [th.light, th.dark] as sw, i (i)}
                    <span class="flex-1 flex items-center justify-center p-1.5" style:background-color={sw.bg}>
                      <span class="w-full h-full rounded-md flex flex-col justify-center gap-1 px-1.5" style:background-color={sw.surface}>
                        <span class="h-1 w-3/4 rounded-full opacity-80" style:background-color={sw.text}></span>
                        <span class="h-1.5 w-1/2 rounded-full" style:background-color={sw.accent}></span>
                      </span>
                    </span>
                  {/each}
                </span>
                <span class="flex items-center justify-between gap-1 px-1 pt-1.5 pb-0.5 text-body-sm {on ? 'font-semibold text-[var(--text)]' : 'text-[var(--text-muted)]'}">
                  {th.label}
                  {#if on}<Check size={13} class="text-[var(--accent)]" />{/if}
                </span>
              </button>
            {/each}
          </div>
        </div>

        <!-- Accent color: only for Classic; a Radix theme brings its own. -->
        {#if themePresetPref === CLASSIC_THEME}
        <div class="mt-5 pt-5 border-t border-[var(--border)]">
          <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2.5">{t("settings.appearance.accent")}</span>
          <div class="flex items-center gap-2.5 flex-wrap">
            {#each accentPresets() as preset (preset.id)}
              <button
                type="button"
                class="size-8 shrink-0 rounded-full transition-transform motion-safe:active:scale-90
                       {accentPref === preset.id ? 'ring-2 ring-offset-2 ring-offset-[var(--surface)] ring-[var(--text)]' : 'hover:scale-110'}"
                style:background-color={preset.swatch}
                onclick={() => pickAccent(preset.id)}
                title={preset.label}
                aria-label={t("settings.appearance.accentAria", { name: preset.label })}
                aria-pressed={accentPref === preset.id}
              >
                {#if accentPref === preset.id}
                  <Check size={14} class="mx-auto text-white drop-shadow-[0_1px_1px_rgba(0,0,0,0.4)]" />
                {/if}
              </button>
            {/each}
          </div>
        </div>
        {/if}

        <!-- Density -->
        <div class="mt-5 pt-5 border-t border-[var(--border)]">
          <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2.5">{t("settings.appearance.density")}</span>
          <div class="inline-flex p-0.5 rounded-lg bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]">
            {#each [["comfortable", t("settings.appearance.comfortable"), Rows3], ["compact", t("settings.appearance.compact"), Rows2]] as [val, label, Icon]}
              {@const IconComp = Icon as typeof Rows3}
              <button
                class="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-body-sm font-medium transition-all
                       {densityPref === val
                  ? 'bg-[var(--surface)] text-[var(--text)] shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
                  : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
                onclick={() => pickDensity(val as Density)}
              >
                <IconComp size={14} />
                {label}
              </button>
            {/each}
          </div>
        </div>

        <!-- Text size -->
        <div class="mt-5 pt-5 border-t border-[var(--border)]">
          <span class="flex items-center gap-1.5 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2.5">
            <Type size={12} /> {t("settings.appearance.textSize")}
          </span>
          <div class="inline-flex p-0.5 rounded-lg bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]">
            {#each [["sm", t("settings.appearance.size.sm")], ["md", t("settings.appearance.size.md")], ["lg", t("settings.appearance.size.lg")]] as [val, label]}
              <button
                class="px-3.5 py-1.5 rounded-md text-body-sm font-medium transition-all
                       {fontScalePref === val
                  ? 'bg-[var(--surface)] text-[var(--text)] shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
                  : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
                onclick={() => pickFontScale(val as FontScale)}
              >
                {label}
              </button>
            {/each}
          </div>
        </div>

        <!-- Motion -->
        <div class="mt-5 pt-5 border-t border-[var(--border)]">
          <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-2.5">{t("settings.appearance.motion")}</span>
          <div class="inline-flex p-0.5 rounded-lg bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]">
            {#each [["system", t("settings.appearance.system"), Monitor], ["reduced", t("settings.appearance.reduced"), ZapOff], ["full", t("settings.appearance.full"), Zap]] as [val, label, Icon]}
              {@const IconComp = Icon as typeof Monitor}
              <button
                class="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-body-sm font-medium transition-all
                       {motionPref === val
                  ? 'bg-[var(--surface)] text-[var(--text)] shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
                  : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
                onclick={() => pickMotion(val as MotionPreference)}
              >
                <IconComp size={14} />
                {label}
              </button>
            {/each}
          </div>
          <p class="text-caption text-[var(--text-muted)] mt-2">
            {t("settings.appearance.motionHint")}
          </p>
        </div>
      </section>

      <!-- ── CONNECTED TOOLS (full width) ─────────────── -->
      <section class="mt-10 animate-reveal delay-250">
        <div class="flex items-center gap-2 mb-1">
          <Plug size={16} class="text-[var(--text-muted)]" />
          <h2 class="text-[1rem] font-semibold text-[var(--text)]">{t("settings.tools.title")}</h2>
        </div>
        <p class="text-body text-[var(--text-muted)] mb-5 leading-relaxed">
          {t("settings.tools.intro")}
        </p>

        <div class="grid sm:grid-cols-2 gap-2.5">
          {#each connectionTemplates as template (template.id)}
            {@const st = toolState(template.id)}
            {@const bot = getToolBot(template.id)}
            <div data-connection-id={template.id} class="flex flex-wrap items-center gap-3.5 p-3.5 rounded-xl bg-[var(--surface)] shadow-[0_1px_2px_rgba(0,0,0,0.06)]">
              <div class="size-10 shrink-0 rounded-lg bg-[var(--bg-subtle)] grid place-items-center text-[var(--text)]">
                <ToolIcon tool={template.id} size={20} />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex flex-wrap items-center gap-2">
                  <span class="text-body font-medium text-[var(--text)] break-all">{bot?.display_name ?? template.name}</span>
                  {#if st === "connected"}
                    <span class="inline-flex items-center gap-1 text-micro font-semibold uppercase tracking-wide
                                 text-[var(--success)] bg-[var(--success-bg)] px-1.5 py-0.5 rounded-full">
                      <span class="size-1.5 rounded-full bg-[var(--success)]"></span> {t("settings.tools.connected")}
                    </span>
                  {:else if st === "disconnected"}
                    <span class="text-micro font-semibold uppercase tracking-wide text-[var(--warn)] bg-[color-mix(in_oklab,var(--warn)_14%,transparent)] px-1.5 py-0.5 rounded-full">
                      {t("settings.tools.disconnected")}
                    </span>
                  {/if}
                </div>
                <p class="text-caption text-[var(--text-muted)] truncate mt-0.5">{bot ? connectionId(bot) : template.description}</p>
              </div>
              <div class="shrink-0 flex items-center gap-1.5">
                {#if st === "connected" && bot}
                  <button
                    class="text-body-sm text-[var(--text-muted)] hover:text-[var(--error)] px-2.5 py-1.5 rounded-md hover:bg-[var(--error-bg)] transition-colors disabled:opacity-50"
                    disabled={busyId === bot.id}
                    onclick={() => handleDisconnect(bot.id)}
                  >
                    {busyId === bot.id ? "…" : t("settings.tools.disconnect")}
                  </button>
                {:else if st === "disconnected" && bot}
                  <button
                    class="text-body-sm text-[var(--text-faint)] hover:text-[var(--error)] px-2 py-1.5 rounded-md hover:bg-[var(--error-bg)] transition-colors disabled:opacity-50"
                    disabled={busyId === bot.id}
                    onclick={() => handleRemove(bot.id)}
                  >
                    {t("common.remove")}
                  </button>
                  <button
                    class="flex items-center gap-1.5 text-body-sm font-medium text-[var(--btn-success-text)]
                           bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
                    onclick={() => openConnect(template)}
                  >
                    {t("settings.tools.reconnect")}
                  </button>
                {:else}
                  <button
                    class="flex items-center gap-1.5 text-body-sm font-medium text-[var(--btn-success-text)]
                           bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors
                           motion-safe:active:scale-[0.97]"
                    onclick={() => openConnect(template)}
                  >
                    {t("settings.tools.connect")}
                  </button>
                {/if}
              </div>
              {#if placesOf(bot).length}
                <div class="basis-full min-w-0 border-t border-[var(--border)] pt-2.5" data-testid="tool-places">
                  <p class="text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1">
                    {t("settings.places.title")}
                  </p>
                  <ul class="flex flex-col">
                    {#each placesOf(bot) as place (place.id)}
                      <li class="flex items-center gap-2 min-w-0 py-1 text-body-sm">
                        <span class="font-mono text-[var(--text)] truncate max-w-[70%] shrink-0">{place.workspace}</span>
                        <span class="text-caption text-[var(--text-faint)] truncate min-w-0 hidden sm:inline">@{place.username}</span>
                        {#if place.last_seen_at}
                          <span class="ml-auto text-caption text-[var(--text-faint)] shrink-0"><TimeAgo date={place.last_seen_at} /></span>
                        {/if}
                        <button
                          class="{place.last_seen_at ? '' : 'ml-auto'} shrink-0 text-caption text-[var(--text-faint)] hover:text-[var(--error)]
                                 px-2 py-1 rounded-md hover:bg-[var(--error-bg)] transition-colors disabled:opacity-50"
                          disabled={busyId === place.id}
                          title={t("settings.places.removeHint")}
                          onclick={() => {
                            if (confirm(t("settings.places.removeConfirm", { place: place.workspace ?? "" }))) handleRemove(place.id);
                          }}
                        >
                          {t("common.remove")}
                        </button>
                      </li>
                    {/each}
                  </ul>
                </div>
              {/if}
            </div>
          {/each}
        </div>
        <details class="mt-4 text-body-sm text-[var(--text-muted)]" data-testid="places-howto">
          <summary class="cursor-pointer text-[var(--text)]">{t("settings.places.howTitle")}</summary>
          <p class="mt-2 leading-relaxed">{t("settings.places.howIntro")}</p>
          <pre class="mt-2 p-3 rounded-lg bg-[var(--bg-subtle)] text-caption font-mono overflow-x-auto whitespace-pre">{`"lific": {
  "type": "http",
  "url": "${location.origin}/mcp",
  "headersHelper": "lific-inbox-hook --headers"
}`}</pre>
          <p class="mt-2 leading-relaxed">{t("settings.places.howOther")}</p>
        </details>
        <button class="mt-4 toolbar-pill" onclick={() => { customOpen = !customOpen; toolsError = ""; }} aria-expanded={customOpen}>
          {t("settings.custom.toggle")}
        </button>
        {#if toolsError}<p role="alert" class="mt-3 text-body-sm text-[var(--error)]">{toolsError}</p>{/if}
        {#if customOpen}
          <form class="mt-4 max-w-lg flex flex-col gap-3" onsubmit={(e) => { e.preventDefault(); connectCustom(); }}>
            <p class="text-body-sm text-[var(--text-muted)]">{t("settings.custom.intro")}</p>
            <label class="text-body-sm">{t("settings.custom.template")}
              <select class="mt-1 w-full rounded-md border border-[var(--border)] bg-[var(--surface)] p-2" bind:value={customTemplate}>
                <option value="">{t("settings.custom.genericClient")}</option>
                {#each TOOL_TEMPLATES as template}<option value={template.id}>{template.name}</option>{/each}
              </select>
            </label>
            <label class="text-body-sm">{t("settings.custom.id")}
              <input required maxlength="48" pattern={"[a-zA-Z0-9][a-zA-Z0-9_\\-]{0,47}"} placeholder="codex-laptop" class="mt-1 w-full rounded-md border border-[var(--border)] bg-[var(--surface)] p-2" bind:value={customId} />
            </label>
            <label class="text-body-sm">{t("settings.custom.name")}
              <input maxlength="80" placeholder={t("settings.custom.namePlaceholder")} class="mt-1 w-full rounded-md border border-[var(--border)] bg-[var(--surface)] p-2" bind:value={customName} />
            </label>
            <button type="submit" class="self-start text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-2 rounded-md">{t("settings.custom.submit")}</button>
          </form>
        {/if}
      </section>

      <!-- ── ACCOUNT (profile + security, bottom of page) ─── -->
      <section class="mt-10 pt-8 border-t border-[var(--border)] animate-reveal delay-250">
        <h2 class="text-[1rem] font-semibold text-[var(--text)] mb-1">{t("settings.account.title")}</h2>
        <p class="text-body text-[var(--text-muted)] mb-6 leading-relaxed">
          {t("settings.account.intro")}
        </p>

        <!-- Profile (proper labeled form) -->
        <div class="max-w-[480px] flex flex-col gap-3.5">
          <label class="block">
            <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1.5">{t("settings.profile.displayName")}</span>
            <input
              bind:value={profileName}
              class="w-full px-3 py-2 text-body rounded-md border border-[var(--border)] bg-[var(--bg)] text-[var(--text)]
                     outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
            />
          </label>
          <label class="block">
            <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1.5">{t("settings.profile.email")}</span>
            <input
              bind:value={profileEmail}
              type="email"
              class="w-full px-3 py-2 text-body rounded-md border border-[var(--border)] bg-[var(--bg)] text-[var(--text)]
                     outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
            />
          </label>
          <div>
            <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1.5">{t("settings.profile.username")}</span>
            <p class="text-body font-mono text-[var(--text-muted)]">@{user.username}</p>
          </div>
        </div>
        {#if profileError}
          <p class="text-caption text-[var(--error)] mt-2.5 flex items-center gap-1"><AlertTriangle size={12} /> {profileError}</p>
        {/if}
        <div class="flex items-center gap-3 mt-4">
          <button
            class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md
                   hover:bg-[var(--btn-success-hover)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            disabled={!hasProfileChanges || profileSaving}
            onclick={saveProfile}
          >
            {profileSaving ? t("common.saving") : t("settings.profile.save")}
          </button>
          {#if profileSavedAt}
            <span class="inline-flex items-center gap-1 text-body-sm text-[var(--success)]" aria-live="polite"><Check size={13} /> {t("common.saved")}</span>
          {/if}
        </div>

        <!-- Password -->
        <div class="mt-8 pt-6 border-t border-[var(--border)]">
          <div class="flex items-center gap-2 mb-3.5">
            <Lock size={15} class="text-[var(--text-muted)]" />
            <h3 class="text-body-lg font-semibold text-[var(--text)]">{t("settings.password.title")}</h3>
          </div>
          <div class="max-w-[480px] flex flex-col gap-2.5">
            <input
              type="password" bind:value={curPw} placeholder={t("settings.password.current")} autocomplete="current-password"
              class="px-3 py-2 text-body rounded-md border border-[var(--border)] bg-[var(--bg)] text-[var(--text)]
                     outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
            />
            <input
              type="password" bind:value={newPw} placeholder={t("settings.password.new")} autocomplete="new-password"
              class="px-3 py-2 text-body rounded-md border border-[var(--border)] bg-[var(--bg)] text-[var(--text)]
                     outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
              onkeydown={(e) => { if (e.key === 'Enter') submitPassword(); }}
            />
          </div>
          <p class="text-caption text-[var(--text-muted)] mt-2.5 max-w-[480px] leading-relaxed">
            {t("settings.password.note")}
          </p>
          {#if pwError}
            <p class="text-caption text-[var(--error)] mt-2 flex items-center gap-1" role="alert"><AlertTriangle size={12} /> {pwError}</p>
          {/if}
          <div class="flex items-center gap-3 mt-3">
            <button
              class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md
                     hover:bg-[var(--btn-success-hover)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
              disabled={pwSaving || !curPw || !newPw}
              onclick={submitPassword}
            >
              {pwSaving ? t("settings.password.saving") : t("settings.password.submit")}
            </button>
            {#if pwSuccess}
              <span class="inline-flex items-center gap-1 text-body-sm text-[var(--success)]" role="status">
                <Check size={13} /> {t("settings.password.changed")}
              </span>
            {/if}
          </div>
        </div>

        <!-- Sessions (sign out) — the very bottom of the page -->
        <div class="mt-8 pt-6 border-t border-[var(--border)]">
          <h3 class="text-body-lg font-semibold text-[var(--text)] mb-1">{t("settings.sessions.title")}</h3>
          <p class="text-body-sm text-[var(--text-muted)] mb-3.5 leading-relaxed">
            {t("settings.sessions.intro")}
          </p>
          <div class="flex flex-wrap items-center gap-2">
            <button
              class="inline-flex items-center gap-1.5 text-body-sm font-medium text-[var(--text)]
                     border border-[var(--border)] px-3 py-1.5 rounded-md
                     hover:bg-[var(--bg-subtle)] transition-colors"
              onclick={logoutNow}
            >
              <LogOut size={14} />
              {t("settings.sessions.signOut")}
            </button>
            <button
              class="inline-flex items-center gap-1.5 text-body-sm text-[var(--error)] border border-[var(--error)]
                     px-3 py-1.5 rounded-md hover:bg-[var(--error-bg)] transition-colors disabled:opacity-50"
              disabled={signingOut}
              aria-expanded={confirmingSignOutAll}
              onclick={() => { confirmingSignOutAll = !confirmingSignOutAll; signOutAllError = ""; }}
            >
              {t("settings.sessions.signOutAll")}
            </button>
          </div>
          <!-- Inline confirm (never a browser confirm()), matching the
               attachment-delete pattern: the consequence is spelled out where
               the second click happens. -->
          {#if confirmingSignOutAll}
            <div
              class="mt-3 max-w-[480px] flex flex-col gap-2 rounded-md border border-[var(--error)]
                     bg-[var(--error-bg)] px-3 py-2.5"
              role="alert"
            >
              <p class="text-caption text-[var(--text)] leading-relaxed">
                {t("settings.sessions.confirm")}
              </p>
              {#if signOutAllError}
                <p class="text-caption text-[var(--error)] flex items-center gap-1">
                  <AlertTriangle size={12} /> {signOutAllError}
                </p>
              {/if}
              <div class="flex items-center gap-2">
                <button
                  class="text-body-sm font-medium text-[var(--error-text)] bg-[var(--error)] px-3 py-1.5 rounded-md
                         transition-opacity hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed"
                  disabled={signingOut}
                  onclick={signOutAll}
                >
                  {signingOut ? t("settings.sessions.revoking") : t("settings.sessions.confirmButton")}
                </button>
                <button
                  class="text-body-sm text-[var(--text-muted)] px-3 py-1.5 rounded-md
                         hover:bg-[var(--bg-subtle)] transition-colors disabled:opacity-50"
                  disabled={signingOut}
                  onclick={() => { confirmingSignOutAll = false; signOutAllError = ""; }}
                >
                  {t("common.cancel")}
                </button>
              </div>
            </div>
          {/if}
        </div>
      </section>
    {/if}
  </div>
</div>

<!-- ── CONNECT MODAL ──────────────────────────────────── -->
{#if connectTool}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-50 bg-black/50 grid place-items-center p-4" onclick={closeConnect}>
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="w-full max-w-[620px] bg-[var(--surface)] border border-[var(--border)] rounded-xl shadow-2xl
             max-h-[85dvh] overflow-y-auto"
      role="dialog"
      aria-modal="true"
      aria-label={t("settings.connect.title", { name: connectTool.name })}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- Header -->
      <div class="flex items-center gap-3 px-5 py-4 border-b border-[var(--border)]">
        <div class="size-9 shrink-0 rounded-lg bg-[var(--bg-subtle)] grid place-items-center text-[var(--text)]">
          <ToolIcon tool={connectTool.id} size={18} />
        </div>
        <div class="flex-1 min-w-0">
          <h3 class="text-body-lg font-semibold text-[var(--text)] leading-tight">{t("settings.connect.title", { name: connectTool.name })}</h3>
          <p class="text-caption text-[var(--text-muted)] truncate">{connectTool.description}</p>
        </div>
        <button class="size-7 grid place-items-center rounded-md text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors disabled:opacity-50" onclick={closeConnect} disabled={connecting || reauthBusy} aria-label={t("common.close")}>
          <X size={16} />
        </button>
      </div>

      <div class="p-5">
        {#if connecting}
          <div class="flex items-center gap-3 py-8 justify-center text-[var(--text-muted)]">
            <div class="size-5 rounded-full border-2 border-[var(--border)] border-t-[var(--accent)] animate-spin"></div>
            <span class="text-body">{t("settings.connect.minting")}</span>
          </div>
        {:else if reauthNeeded}
          <!-- The session is fine; it is just older than the window a durable
               credential needs. Say that, and give them the one field that
               fixes it, rather than an error with nowhere to go. -->
          <div class="flex flex-col gap-3">
            <div class="flex items-start gap-2.5 text-body text-[var(--text)] bg-[var(--bg-subtle)] px-3.5 py-3 rounded-md">
              <Lock size={16} class="shrink-0 mt-0.5 text-[var(--text-muted)]" />
              <p class="leading-relaxed">
                {t("settings.connect.reauth", { name: connectTool.name })}
              </p>
            </div>
            {#if autoReauthNote}
              <p class="text-caption text-[var(--text-muted)]" role="status">
                {t("settings.connect.autoFailed", { reason: autoReauthNote })}
              </p>
            {/if}
            {#if connectError}
              <p class="text-caption text-[var(--error)] flex items-center gap-1" role="alert">
                <AlertTriangle size={12} /> {connectError}
              </p>
            {/if}
            <label class="block">
              <span class="block text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)] mb-1.5">
                {t("settings.password.current")}
              </span>
              <input
                type="password"
                bind:value={reauthPassword}
                autocomplete="current-password"
                disabled={reauthBusy}
                class="w-full px-3 py-2 text-body rounded-md border border-[var(--border)] bg-[var(--bg)] text-[var(--text)]
                       outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] disabled:opacity-50"
                onkeydown={(e) => { if (e.key === 'Enter') submitReauthAndConnect(); }}
              />
            </label>
            <div class="flex items-center gap-2">
              <button
                class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md
                       hover:bg-[var(--btn-success-hover)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                disabled={reauthBusy || !reauthPassword}
                onclick={submitReauthAndConnect}
              >
                {reauthBusy ? t("settings.connect.verifying") : t("settings.connect.confirm", { name: connectTool.name })}
              </button>
              <button
                class="text-body-sm text-[var(--text-muted)] px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] transition-colors disabled:opacity-50"
                disabled={reauthBusy}
                onclick={closeConnect}
              >
                {t("common.cancel")}
              </button>
            </div>
          </div>
        {:else if connectError}
          <div class="flex items-start gap-2.5 text-body text-[var(--error)] bg-[var(--error-bg)] px-3.5 py-3 rounded-md" role="alert">
            <AlertTriangle size={16} class="shrink-0 mt-0.5" />
            <span>{connectError}</span>
          </div>
        {:else if connectKey}
          {@const stepCount = connectTool.usesEnvKey ? 3 : 2}

          <!-- ── STEP 1 · API KEY ───────────────────────────────
               Shown once; hidden behind a mask by default. Surfaced as its
               own first-class step so env-var tools (Pi, Codex) never bury
               the secret inside a config the user might not read. -->
          <section class="mb-5">
            <div class="flex items-center gap-2.5 mb-2">
              <span class="size-5 shrink-0 grid place-items-center rounded-full bg-[var(--btn-success)] text-[var(--btn-success-text)] text-micro font-bold tabular-nums">1</span>
              <h4 class="flex items-center gap-1.5 text-body font-semibold text-[var(--text)]">
                <KeyRound size={14} class="text-[var(--text-muted)]" /> {t("settings.connect.step.key")}
              </h4>
            </div>

            <div class="rounded-lg border border-[var(--btn-success)] bg-[color-mix(in_oklab,var(--btn-success)_8%,var(--bg))] overflow-hidden">
              <div class="flex items-center gap-2 px-3 py-2.5">
                <code
                  class="flex-1 min-w-0 text-body-sm font-mono text-[var(--text)] overflow-x-auto whitespace-nowrap leading-none py-0.5
                         {keyRevealed ? '' : 'select-none cursor-default'}"
                >{displayKey}</code>
                <button
                  class="shrink-0 size-7 grid place-items-center rounded-md text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors"
                  onclick={() => (keyRevealed = !keyRevealed)}
                  title={keyRevealed ? t("settings.connect.hideKey") : t("settings.connect.revealKey")}
                  aria-label={keyRevealed ? t("settings.connect.hideKey") : t("settings.connect.revealKey")}
                >
                  {#if keyRevealed}<EyeOff size={15} />{:else}<Eye size={15} />{/if}
                </button>
              </div>
              <button
                class="w-full flex items-center justify-center gap-1.5 py-2 text-body-sm font-semibold border-t border-[color-mix(in_oklab,var(--btn-success)_35%,transparent)] transition-colors
                       {keyCopied
                  ? 'bg-[var(--success-bg)] text-[var(--success)]'
                  : 'bg-[var(--btn-success)] text-[var(--btn-success-text)] hover:bg-[var(--btn-success-hover)]'}"
                onclick={copyKey}
              >
                {#if keyCopied}<Check size={14} /> {t("settings.connect.keyCopied")}{:else}<Copy size={14} /> {t("settings.connect.copyKey")}{/if}
              </button>
            </div>
          </section>

          <!-- ── STEP 2 · CONFIG ────────────────────────────────── -->
          <section class="mb-5">
            <div class="flex items-center gap-2.5 mb-2">
              <span class="size-5 shrink-0 grid place-items-center rounded-full bg-[var(--btn-success)] text-[var(--btn-success-text)] text-micro font-bold tabular-nums">2</span>
              <h4 class="flex items-center gap-1.5 text-body font-semibold text-[var(--text)]">
                <FileCode2 size={14} class="text-[var(--text-muted)]" /> {t("settings.connect.step.config")}
              </h4>
            </div>

            <!-- OS selector: its own full-width segmented row. OSes that share
                 an identical path are merged into one button (e.g. Linux / macOS).
                 A single group means the path is the same everywhere. -->
            {#if osGroups.length > 0}
            <div class="flex items-center gap-2 mb-2.5">
              <div class="inline-flex flex-1 p-0.5 rounded-lg bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.08)]">
                {#each osGroups as group (group.label)}
                  {@const active = group.oses.includes(selectedOs)}
                  <button
                    class="flex-1 px-2 py-1.5 rounded-md text-caption font-medium transition-all
                           {active
                      ? 'bg-[var(--btn-success)] text-[var(--btn-success-text)] shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
                      : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
                    onclick={() => (selectedOs = group.oses[0])}
                  >
                    {group.label}
                  </button>
                {/each}
              </div>
              {#if !pathsDiffer}
                <span class="text-micro text-[var(--text-muted)] shrink-0">{t("settings.connect.sameEverywhere")}</span>
              {/if}
            </div>

            <div class="flex items-center gap-2 mb-2.5">
              <span class="text-micro font-semibold uppercase tracking-wide text-[var(--text-muted)] shrink-0">{t("settings.connect.file")}</span>
              <code class="flex-1 min-w-0 font-mono text-caption bg-[var(--bg-subtle)] px-2 py-1 rounded text-[var(--text)] overflow-x-auto whitespace-nowrap">{activePath}</code>
            </div>
            {/if}

            {#if connectTool.configNote}
              <div class="flex flex-col gap-1.5 mb-2.5">
                {#each connectTool.configNote as step, i (i)}
                  {#if step.text}
                    <p class="text-body-sm text-[var(--text)] leading-snug">{step.text}</p>
                  {/if}
                  {#if step.command}
                    <!-- Same shape as the API-key card: content row on top,
                         full-width copy bar underneath. No floating button
                         overlapping a single line of scrolling text. -->
                    <div class="rounded-lg bg-[var(--bg)] overflow-hidden ring-1 ring-[var(--border)]">
                      <pre class="px-3 py-2.5 text-caption font-mono text-[var(--text)] overflow-x-auto whitespace-pre">{step.command}</pre>
                      <!-- Separator matches the bar's own bg so no seam fights
                           the rounded container. Green-tinted bar + green text:
                           lively and on-brand without a loud solid button. -->
                      <button
                        class="w-full flex items-center justify-center gap-1.5 py-2 text-caption font-semibold transition-colors
                               {noteCopiedIdx === i
                          ? 'bg-[var(--success-bg)] text-[var(--success)]'
                          : 'bg-[color-mix(in_oklab,var(--btn-success)_14%,var(--bg))] text-[var(--success)] hover:bg-[color-mix(in_oklab,var(--btn-success)_22%,var(--bg))]'}"
                        onclick={() => copyNote(i, step.command!)}
                      >
                        {#if noteCopiedIdx === i}<Check size={13} /> {t("common.copied")}{:else}<Copy size={13} /> {t("settings.connect.copyCommand")}{/if}
                      </button>
                    </div>
                  {/if}
                {/each}
              </div>
            {/if}

            <div class="relative">
              <pre
                class="bg-[var(--bg)] border border-[var(--border)] rounded-lg p-3.5 pr-12 text-caption font-mono text-[var(--text)] overflow-x-auto leading-relaxed
                       {keyRevealed ? '' : 'select-none'}"
              >{displayConfig}</pre>
              <button
                class="absolute top-2 right-2 inline-flex items-center gap-1 text-micro font-semibold
                       px-2 py-1 rounded-md bg-[var(--surface)] border border-[var(--border)]
                       {copied ? 'text-[var(--success)]' : 'text-[var(--text-muted)] hover:text-[var(--text)]'} transition-colors"
                onclick={copyConfig}
              >
                {#if copied}<Check size={12} /> {t("common.copied")}{:else}<Copy size={12} /> {t("common.copy")}{/if}
              </button>
            </div>
          </section>

          <!-- ── STEP 3 · ENV VAR (env-key tools only) ──────────── -->
          {#if connectTool.usesEnvKey}
            <section class="mb-5">
              <div class="flex items-center gap-2.5 mb-2">
                <span class="size-5 shrink-0 grid place-items-center rounded-full bg-[var(--btn-success)] text-[var(--btn-success-text)] text-micro font-bold tabular-nums">3</span>
                <h4 class="flex items-center gap-1.5 text-body font-semibold text-[var(--text)]">
                  <Terminal size={14} class="text-[var(--text-muted)]" /> {t("settings.connect.step.env")}
                  <span class="font-normal text-[var(--text-muted)]">· {OS_LABELS[selectedOs]}</span>
                </h4>
              </div>
              <div class="relative">
                <pre
                  class="bg-[var(--bg)] border border-[var(--border)] rounded-lg p-3 pr-12 text-caption font-mono text-[var(--text)] overflow-x-auto
                         {keyRevealed ? '' : 'select-none'}"
                >{keyRevealed ? exportLine : exportLine.split(connectKey).join(maskedKey)}</pre>
                <button
                  class="absolute top-2 right-2 inline-flex items-center gap-1 text-micro font-semibold
                         px-2 py-1 rounded-md bg-[var(--surface)] border border-[var(--border)]
                         {exportCopied ? 'text-[var(--success)]' : 'text-[var(--text-muted)] hover:text-[var(--text)]'} transition-colors"
                  onclick={copyExport}
                >
                  {#if exportCopied}<Check size={12} /> {t("common.copied")}{:else}<Copy size={12} /> {t("common.copy")}{/if}
                </button>
              </div>
              <p class="text-micro text-[var(--text-muted)] mt-1.5 leading-relaxed">{persistHint}</p>
            </section>
          {/if}

          <p class="text-micro text-[var(--text-muted)] text-center leading-relaxed">
            {t("settings.connect.shownOnce")}
          </p>
        {/if}
      </div>

      <div class="flex justify-end px-5 py-3.5 border-t border-[var(--border)]">
        <button
          class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-4 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
          onclick={closeConnect}
        >
          {t("common.done")}
        </button>
      </div>
    </div>
  </div>
{/if}
