<script lang="ts">
  import { listUsers, listProjectGroups, type UserSummary, type ProjectGroup } from "./api";
  import IconPicker from "./IconPicker.svelte";
  import Select from "./Select.svelte";
  import { t, locale } from "./i18n";

  let {
    name = $bindable(""),
    identifier = $bindable(""),
    description = $bindable(""),
    emoji = $bindable(""),
    leadUserId = $bindable<number | null>(null),
    groupId = $bindable<number | null>(null),
    mode = "create",
    identifierLocked = false,
  }: {
    name?: string;
    identifier?: string;
    description?: string;
    emoji?: string;
    leadUserId?: number | null;
    /** Sidebar group to file the project into. The form only collects it —
     *  persisting it is the caller's job, since it rides a separate endpoint. */
    groupId?: number | null;
    mode?: "create" | "edit";
    identifierLocked?: boolean;
  } = $props();

  let users = $state<UserSummary[]>([]);
  let groups = $state<ProjectGroup[]>([]);
  let identifierTouched = $state(false);

  $effect(() => {
    listUsers().then((res) => {
      if (res.ok) users = res.data;
    });
    listProjectGroups().then((res) => {
      if (res.ok) groups = res.data;
    });
  });

  // Auto-generate identifier from name (create mode only)
  $effect(() => {
    if (mode === "create" && !identifierTouched && name) {
      identifier = name
        .toUpperCase()
        .replace(/[^A-Z0-9]+/g, "")
        .slice(0, 5);
    }
  });

  // Playful example names; one is picked per mount (projects.form.example.1..13).
  const PLACEHOLDER_COUNT = 13;
  const placeholder = t(`projects.form.example.${1 + Math.floor(Math.random() * PLACEHOLDER_COUNT)}`);

  let previewId = $derived((identifier.trim().toUpperCase() || "PRO") + "-1");

  let groupOptions = $derived([
    { value: null, label: t("projects.form.noGroup") },
    ...groups.map((g) => ({ value: g.id, label: g.name })),
  ]);

  let userOptions = $derived([
    { value: null, label: t("projects.form.noLead") },
    ...users.map((u) => ({
      value: u.id,
      label: u.display_name || u.username,
      username: u.username,
      is_admin: u.is_admin,
      created_at: u.created_at,
    })),
  ]);

  function formatMemberSince(iso: string): string {
    const d = new Date(iso + "Z");
    return d.toLocaleDateString(locale(), { month: "short", year: "numeric" });
  }

  function userInitials(name: string): string {
    return name.split(/[\s_-]+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? "").join("");
  }
</script>

<div class="max-w-[560px] mx-auto px-6 pt-12 md:pt-16">
  <!-- Name -->
  <div class="mb-8">
    <label
      for="project-name"
      class="block text-body-sm font-medium text-[var(--text)] mb-2 w-fit"
    >
      {t("projects.form.name")}
    </label>
    <input
      id="project-name"
      type="text"
      bind:value={name}
      class="w-full rounded-md px-3 py-2.5 text-body-lg
             border border-[var(--border)] bg-[var(--bg-subtle)]
             text-[var(--text)] placeholder:text-[var(--text-faint)]
             outline-none transition-colors
             focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_var(--accent-subtle)]"
      placeholder={mode === "create" ? placeholder : ""}
      autofocus={mode === "create"}
    />
  </div>

  <!-- Identifier + Lead + Icon row -->
  <div class="flex gap-5 items-start mb-8">
    <div class="shrink-0">
      <label
        for="project-id"
        class="block text-body-sm font-medium text-[var(--text)] mb-2 w-fit"
      >
        {t("projects.form.identifier")}
      </label>
      <input
        id="project-id"
        type="text"
        bind:value={identifier}
        oninput={() => { identifierTouched = true; }}
        disabled={identifierLocked}
        class="w-[120px] rounded-md px-3 py-2.5 text-body-lg font-mono
               uppercase tracking-wide
               border border-[var(--border)] bg-[var(--bg-subtle)]
               text-[var(--text)] placeholder:text-[var(--text-faint)]
               outline-none transition-colors
               focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_var(--accent-subtle)]
               disabled:opacity-50 disabled:cursor-not-allowed"
        placeholder="PRO"
        maxlength="5"
        spellcheck="false"
        autocapitalize="characters"
      />
      <p class="mt-1.5 text-body-sm text-[var(--text-faint)] w-fit">
        {mode === "edit" ? t("projects.form.issuePrefix") : t("projects.form.issuesBecome")}
      </p>
      <span
        class="inline-block font-mono text-caption font-medium
               text-[var(--accent)] bg-[var(--accent-subtle)]
               px-1.5 py-0.5 rounded mt-0.5"
      >
        {previewId}
      </span>
    </div>

    <div class="flex-1">
      <label
        class="block text-body-sm font-medium text-[var(--text)] mb-2"
      >
        {t("projects.form.lead")}
      </label>
      <Select
        options={userOptions}
        bind:value={leadUserId}
        placeholder={t("projects.form.noLead")}
        renderSelected={selectedUser}
        renderOption={userOption}
      />
    </div>

    <div>
      <label
        class="block text-body-sm font-medium text-[var(--text)] mb-2 w-fit"
      >
        {t("projects.form.icon")}
      </label>
      <IconPicker value={emoji} onchange={(v) => { emoji = v; }} />
    </div>
  </div>

  {#if groups.length > 0}
    <!-- Sidebar grouping. Personal to the signed-in user, unlike Lead. -->
    <div class="mb-8 max-w-xs">
      <label class="block text-body-sm font-medium text-[var(--text)] mb-2 w-fit">
        {t("projects.form.group")}
      </label>
      <Select options={groupOptions} bind:value={groupId} placeholder={t("projects.form.noGroup")} />
    </div>
  {/if}

  <!-- Description -->
  <div class="mb-8">
    <div class="flex items-baseline gap-2 mb-2">
      <label
        for="project-desc"
        class="text-body-sm font-medium text-[var(--text)]"
      >
        {t("projects.form.description")}
      </label>
      <span class="text-caption text-[var(--text-faint)]">{t("projects.form.optional")}</span>
    </div>
    <textarea
      id="project-desc"
      bind:value={description}
      class="w-full rounded-md px-3 py-2.5 text-body-lg min-h-[100px]
             border border-[var(--border)] bg-[var(--bg-subtle)]
             text-[var(--text)] placeholder:text-[var(--text-faint)]
             outline-none resize-y transition-colors
             focus:border-[var(--accent)] focus:shadow-[0_0_0_3px_var(--accent-subtle)]"
      placeholder={t("projects.form.descriptionPlaceholder")}
      rows="3"
    ></textarea>
  </div>
</div>

{#snippet selectedUser(opt: { value: string | number | null; label: string; [key: string]: unknown })}
  <div class="flex items-center gap-2">
    {#if opt.value !== null}
      <div
        class="size-5 rounded-full bg-[var(--accent)] text-[var(--accent-text)]
               flex items-center justify-center text-micro font-semibold shrink-0"
      >
        {userInitials(opt.label)}
      </div>
    {/if}
    <span class="text-body-lg text-[var(--text)]">{opt.label}</span>
  </div>
{/snippet}

{#snippet userOption(opt: { value: string | number | null; label: string; [key: string]: unknown }, isSelected: boolean)}
  {#if opt.value === null}
    <span class="text-body text-[var(--text-faint)]">{opt.label}</span>
  {:else}
    <div class="flex items-center gap-2.5">
      <div
        class="size-7 rounded-full flex items-center justify-center
               text-micro font-semibold shrink-0
               {isSelected
          ? 'bg-[var(--accent)] text-[var(--accent-text)]'
          : 'bg-[var(--bg-subtle)] text-[var(--text-muted)]'}"
      >
        {userInitials(opt.label)}
      </div>
      <div class="min-w-0">
        <div class="flex items-center gap-1.5">
          <span
            class="text-body truncate
                   {isSelected ? 'text-[var(--accent)] font-medium' : 'text-[var(--text)]'}"
          >
            {opt.label}
          </span>
          {#if opt.is_admin}
            <span
              class="text-micro font-semibold uppercase tracking-wide
                     px-1 py-0.5 rounded bg-[var(--accent-subtle)] text-[var(--accent)]"
            >
              {t("projects.form.admin")}
            </span>
          {/if}
        </div>
        <span class="text-caption text-[var(--text-faint)]">
          {t("projects.form.memberSince", { date: formatMemberSince(opt.created_at as string) })}
        </span>
      </div>
    </div>
  {/if}
{/snippet}
