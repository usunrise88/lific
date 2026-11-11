// Fork: where the sidebar logo leads, an instance setting. Loaded once by
// the shell; Instance settings updates it in place so a change applies
// without a reload.

import { getInstance, type LogoTarget } from "./api";

export { REPO_URL, logoRoute } from "./logoRoute";

class LogoTargetState {
  value = $state<LogoTarget>("repo");
}

export const logoTarget = new LogoTargetState();

let loaded = false;

export async function loadLogoTarget(): Promise<void> {
  if (loaded) return;
  loaded = true;
  const res = await getInstance();
  if (res.ok && res.data.logo_target) logoTarget.value = res.data.logo_target;
}
