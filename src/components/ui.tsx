// Shared building blocks. Pages compose these instead of styling from scratch.
import type { LucideIcon } from "lucide-react";
import { AlertTriangle, Info, X } from "lucide-react";
import type { ReactNode } from "react";

export function Button({
  children,
  onClick,
  variant,
  size,
  disabled,
  title,
  icon: Icon,
  type = "button",
}: {
  children?: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "danger" | "ghost";
  size?: "small" | "large" | "icon";
  disabled?: boolean;
  title?: string;
  icon?: LucideIcon;
  type?: "button" | "submit";
}) {
  return (
    <button
      type={type}
      className={["btn", variant, size].filter(Boolean).join(" ")}
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={title}
    >
      {Icon && <Icon size={size === "small" ? 14 : 16} strokeWidth={1.75} />}
      {children}
    </button>
  );
}

export function Toggle({ on, onChange, disabled, label }: { on: boolean; onChange: (on: boolean) => void; disabled?: boolean; label?: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      className={`toggle ${on ? "on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!on)}
    />
  );
}

export function Select<T extends string>({
  value,
  options,
  onChange,
  disabled,
  width,
}: {
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
  disabled?: boolean;
  width?: number;
}) {
  return (
    <select className="select" value={value} disabled={disabled} style={{ width }} onChange={(e) => onChange(e.target.value as T)}>
      {options.map(([v, label]) => (
        <option key={v} value={v}>
          {label}
        </option>
      ))}
    </select>
  );
}

export function Segmented<T extends string>({ value, options, onChange }: { value: T; options: [T, string][]; onChange: (value: T) => void }) {
  return (
    <div className="segmented" role="radiogroup">
      {options.map(([v, label]) => (
        <button key={v} type="button" role="radio" aria-checked={value === v} className={value === v ? "on" : ""} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

/** A colored icon tile, like BetterWispr's SymbolTile. */
export function Tile({ icon: Icon, color, size = 28 }: { icon: LucideIcon; color: string; size?: number }) {
  return (
    <span className="tile" style={{ width: size, height: size, background: color, borderRadius: size * 0.27 }}>
      <Icon size={size * 0.55} strokeWidth={2} />
    </span>
  );
}

export function Card({ children, pad, className }: { children: ReactNode; pad?: boolean; className?: string }) {
  return <div className={["card", pad && "card-pad", className].filter(Boolean).join(" ")}>{children}</div>;
}

/** A titled group of rows inside one card, like a Windows Settings section. */
export function Section({ title, action, children, footer }: { title?: string; action?: ReactNode; children: ReactNode; footer?: ReactNode }) {
  return (
    <section className="section">
      {(title || action) && (
        <div className="section-title">
          {title && <h2>{title}</h2>}
          {action}
        </div>
      )}
      <div className="card rows">{children}</div>
      {footer && <div className="faint" style={{ padding: "0 2px" }}>{footer}</div>}
    </section>
  );
}

/** A label on the left with an optional description, and a control on the right. */
export function Row({ title, detail, icon, children }: { title: ReactNode; detail?: ReactNode; icon?: ReactNode; children?: ReactNode }) {
  return (
    <div className="row">
      {icon}
      <div className="row-label">
        <div>{title}</div>
        {detail && <span className="faint">{detail}</span>}
      </div>
      {children && <div className="row-control">{children}</div>}
    </div>
  );
}

export function Badge({ children, tone }: { children: ReactNode; tone?: "accent" | "success" | "warning" }) {
  return <span className={["badge", tone].filter(Boolean).join(" ")}>{children}</span>;
}

export function Progress({ value }: { value: number }) {
  return (
    <div className="progress" role="progressbar" aria-valuenow={Math.round(value * 100)} aria-valuemin={0} aria-valuemax={100}>
      <div style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%` }} />
    </div>
  );
}

export function Banner({ tone, children, onClose }: { tone: "error" | "warning" | "info"; children: ReactNode; onClose?: () => void }) {
  const Icon = tone === "info" ? Info : AlertTriangle;
  return (
    <div className={`banner ${tone}`} role={tone === "error" ? "alert" : "status"}>
      <Icon size={16} style={{ marginTop: 2, flexShrink: 0 }} />
      <div style={{ flex: 1 }} className="selectable">
        {children}
      </div>
      {onClose && <Button variant="ghost" size="icon" icon={X} title="Dismiss" onClick={onClose} />}
    </div>
  );
}

export function Empty({ icon: Icon, title, children }: { icon: LucideIcon; title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <Icon size={32} strokeWidth={1.5} />
      <h2 style={{ color: "var(--text)" }}>{title}</h2>
      {children}
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <span className="kbd">{children}</span>;
}

/** Shows a shortcut like "Ctrl + Win" as separate keycaps. */
export function ShortcutKeys({ name }: { name: string }) {
  return (
    <span className="hstack" style={{ gap: 4 }}>
      {name.split(" + ").map((key, i) => (
        <Kbd key={i}>{key}</Kbd>
      ))}
    </span>
  );
}

export function Modal({
  title,
  children,
  footer,
  onClose,
}: {
  title: string;
  children: ReactNode;
  footer: ReactNode;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title}>
        <div className="modal-body">
          <h1 style={{ fontSize: 20, lineHeight: "28px" }}>{title}</h1>
          {children}
        </div>
        <div className="modal-footer">{footer}</div>
      </div>
    </div>
  );
}

export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <span className="faint">{hint}</span>}
    </label>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="page-header">
      <div>
        <h1>{title}</h1>
        {subtitle && <p className="subtitle">{subtitle}</p>}
      </div>
      {actions && <div className="hstack">{actions}</div>}
    </div>
  );
}
