import {
  claimMermaidBudget,
  mermaidIsTooComplex,
  type MermaidBudget,
} from "./mermaidLimits";
import { t } from "./i18n";

function showMermaidMessage(block: HTMLDivElement, message: string): void {
  block.style.color = "var(--error)";
  block.style.whiteSpace = "pre-wrap";
  block.style.margin = "0";
  block.textContent = message;
  block.dataset.rendered = "error";
}

export async function renderMermaidBlock(
  block: HTMLDivElement,
  render: (id: string, source: string) => Promise<{ svg: string }>,
  budget: MermaidBudget,
  cancelled: () => boolean,
): Promise<void> {
  // A superseded pass must neither charge the budget nor write to the block.
  // Its node may already be detached (the content changed, or the whole body
  // was remounted), and every byte it claimed would be charged to a render
  // whose output nobody will ever see — permanently starving the next one.
  if (cancelled()) return;

  let source: string;
  try {
    source = decodeURIComponent(block.dataset.mermaid ?? "");
  } catch {
    showMermaidMessage(block, t("markdown.mermaid.malformed"));
    return;
  }

  if (mermaidIsTooComplex(source)) {
    showMermaidMessage(block, t("markdown.mermaid.tooComplex"));
    return;
  }

  const budgetError = claimMermaidBudget(
    new TextEncoder().encode(source).byteLength,
    budget,
  );
  if (budgetError) {
    showMermaidMessage(
      block,
      budgetError === "blocks"
        ? t("markdown.mermaid.tooMany")
        : t("markdown.mermaid.tooMuchSource"),
    );
    return;
  }

  try {
    const id = `mmd-${Math.random().toString(36).slice(2)}`;
    const { svg } = await render(id, source);
    if (!cancelled()) {
      block.innerHTML = svg;
      block.dataset.rendered = "true";
    }
  } catch (error) {
    if (!cancelled()) showMermaidMessage(block, t("markdown.mermaid.error", { error: String(error) }));
  }
}
