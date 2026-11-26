// MiraclePay shared UI helpers — Waves 154/157/197 (DESIGN.md principle 7:
// one pattern per state). Plain functions used by the JS-rendered
// dashboard markup, which cannot use .astro components.

/** Single status→badge taxonomy (design-audit/05 §4). Every page must
 *  use this instead of hand-rolled mappings. Colour is never the only
 *  signal: the label is always rendered as text. */
const BADGES: Record<string, { cls: string; label: string }> = {
  succeeded: { cls: "badge-success", label: "Succeeded" },
  completed: { cls: "badge-success", label: "Completed" },
  paid: { cls: "badge-success", label: "Paid" },
  active: { cls: "badge-success", label: "Active" },
  delivered: { cls: "badge-success", label: "Delivered" },
  verified: { cls: "badge-success", label: "Verified" },
  pending: { cls: "badge-pending", label: "Pending" },
  processing: { cls: "badge-pending", label: "Processing" },
  submitted: { cls: "badge-pending", label: "Submitted" },
  trialing: { cls: "badge-pending", label: "Trialing" },
  queued: { cls: "badge-pending", label: "Queued" },
  failed: { cls: "badge-failed", label: "Failed" },
  expired: { cls: "badge-failed", label: "Expired" },
  rejected: { cls: "badge-failed", label: "Rejected" },
  suspended: { cls: "badge-failed", label: "Suspended" },
  refunded: { cls: "badge-info", label: "Refunded" },
  partially_refunded: { cls: "badge-info", label: "Partially refunded" },
  cancelled: { cls: "badge", label: "Cancelled" },
  canceled: { cls: "badge", label: "Cancelled" },
  draft: { cls: "badge", label: "Draft" },
  disabled: { cls: "badge", label: "Disabled" },
  open: { cls: "badge-pending", label: "Open" },
  sent: { cls: "badge-info", label: "Sent" },
  issued: { cls: "badge-pending", label: "Issued" },
  overdue: { cls: "badge-failed", label: "Overdue" },
  void: { cls: "badge", label: "Void" },
  delivered: { cls: "badge-success", label: "Delivered" },
  queued: { cls: "badge-pending", label: "Queued" },
  retrying: { cls: "badge-pending", label: "Retrying" },
  dead_letter: { cls: "badge-failed", label: "Dead letter" },
  deadletter: { cls: "badge-failed", label: "Dead letter" },
  voided: { cls: "badge", label: "Void" },
  resolved: { cls: "badge-success", label: "Resolved" },
};

export function statusBadge(status: string): string {
  const b = BADGES[status] ?? { cls: "badge", label: status.replaceAll("_", " ") };
  return `<span class="badge ${b.cls}">${b.label}</span>`;
}

/** Toast — the one home for success feedback. Auto-dismisses. */
export function toast(message: string, kind: "success" | "error" = "success"): void {
  const region = document.getElementById("toast-region");
  if (!region) return;
  const el = document.createElement("div");
  el.className = kind === "error" ? "toast toast-error" : "toast";
  el.textContent = message;
  region.appendChild(el);
  window.setTimeout(() => el.remove(), 5000);
}

/** Busy button: disable + spinner + busy label while an async action
 *  runs; always restores. Use instead of ad-hoc label juggling. */
export async function withBusy<T>(
  btn: HTMLButtonElement,
  busyLabel: string,
  fn: () => Promise<T>,
): Promise<T> {
  const original = btn.innerHTML;
  btn.disabled = true;
  btn.innerHTML = "";
  const spin = document.createElement("span");
  spin.className = "spinner";
  btn.append(spin, document.createTextNode(busyLabel));
  try {
    return await fn();
  } finally {
    btn.disabled = false;
    btn.innerHTML = original;
  }
}

/** Skeleton block shown after a 150ms delay (avoids flash on warm
 *  loads). Returns a cancel function; call the returned `done()`
 *  when real content renders. */
export function skeletonAfter(container: HTMLElement, rows = 3): () => void {
  const timer = window.setTimeout(() => {
    const wrap = document.createElement("div");
    wrap.className = "stack";
    wrap.dataset.skeleton = "1";
    wrap.setAttribute("aria-hidden", "true");
    for (let i = 0; i < rows; i++) {
      const s = document.createElement("div");
      s.className = "skeleton";
      s.style.height = "2.5rem";
      wrap.appendChild(s);
    }
    container.appendChild(wrap);
  }, 150);
  return () => {
    window.clearTimeout(timer);
    container.querySelector("[data-skeleton]")?.remove();
  };
}

/** Copy text with feedback (recipient numbers, links, keys). */
export async function copyText(text: string, btn?: HTMLButtonElement): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast("Copied to clipboard.");
    if (btn) {
      const original = btn.textContent;
      btn.textContent = "Copied ✓";
      window.setTimeout(() => (btn.textContent = original), 1600);
    }
  } catch {
    toast("Copy failed — select the text manually.", "error");
  }
}

/** Confirm dialog (native <dialog>) replacing window.confirm. */
export function confirmAction(message: string, confirmLabel = "Confirm"): Promise<boolean> {
  return new Promise((resolve) => {
    const dlg = document.createElement("dialog");
    dlg.className = "mp-dialog";
    const p = document.createElement("p");
    p.textContent = message;
    const row = document.createElement("div");
    row.className = "row";
    row.style.justifyContent = "flex-end";
    const cancel = document.createElement("button");
    cancel.className = "btn btn-ghost";
    cancel.textContent = "Cancel";
    const ok = document.createElement("button");
    ok.className = "btn btn-danger";
    ok.textContent = confirmLabel;
    cancel.addEventListener("click", () => { dlg.close(); resolve(false); });
    ok.addEventListener("click", () => { dlg.close(); resolve(true); });
    dlg.addEventListener("cancel", () => resolve(false));
    row.append(cancel, ok);
    dlg.append(p, row);
    document.body.appendChild(dlg);
    dlg.showModal();
    ok.focus();
  });
}

/** Field-level error: sets aria-invalid + a .field-error under the input. */
export function fieldError(input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement, message: string): void {
  input.setAttribute("aria-invalid", "true");
  const id = `${input.id || input.name}-error`;
  let el = document.getElementById(id);
  if (!el) {
    el = document.createElement("p");
    el.className = "field-error";
    el.id = id;
    input.insertAdjacentElement("afterend", el);
  }
  el.textContent = message;
  input.setAttribute("aria-describedby", id);
}
export function clearFieldError(input: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement): void {
  input.removeAttribute("aria-invalid");
  document.getElementById(`${input.id || input.name}-error`)?.remove();
}

/** Scroll helper gated on reduced motion (design-audit/08). */
export function scrollToEl(el: Element): void {
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  el.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "center" });
}

/** WCAG relative luminance + contrast for a #rrggbb colour. */
export function luminanceOf(hex: string): number {
  const n = parseInt(hex.replace('#', ''), 16);
  const f = (v: number): number => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * f((n >> 16) & 255) + 0.7152 * f((n >> 8) & 255) + 0.0722 * f(n & 255);
}

export function contrastRatio(hexA: string, hexB: string): number {
  const [hi, lo] = [luminanceOf(hexA), luminanceOf(hexB)].sort((a, b) => b - a);
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * Merchant brand accent gate (Wave 187): a merchant colour may theme
 * the payer button only when white text on it keeps >= 3:1 contrast
 * (large/bold control text); otherwise null -> platform default.
 */
export function brandAccent(hex: string | null | undefined): string | null {
  if (!hex || !/^#[0-9a-fA-F]{6}$/.test(hex)) return null;
  // Pure white has luminance 1: contrast = 1.05 / (lum + 0.05).
  return 1.05 / (luminanceOf(hex) + 0.05) >= 3 ? hex : null;
}
