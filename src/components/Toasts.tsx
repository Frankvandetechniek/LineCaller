import { IconAlert, IconCheck, IconX } from "./Icons";

export interface Toast {
  id: number;
  kind: "info" | "warn" | "error";
  text: string;
}

export function Toasts({ toasts, onDismiss }: { toasts: Toast[]; onDismiss: (id: number) => void }) {
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className="toast" data-kind={t.kind} role={t.kind === "error" ? "alert" : "status"}>
          {t.kind === "info" ? <IconCheck size={16} /> : <IconAlert size={16} />}
          <span className="sr-only">{t.kind === "error" ? "Error: " : t.kind === "warn" ? "Warning: " : ""}</span>
          <span className="toast-text">{t.text}</span>
          <button type="button" className="icon-btn small" aria-label="Dismiss" onClick={() => onDismiss(t.id)}><IconX size={14} /></button>
        </div>
      ))}
    </div>
  );
}
