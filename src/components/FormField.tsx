import type { ReactNode } from "react";

interface Props {
  label: string;
  htmlFor: string;
  hint?: string;
  error?: string;
  children: ReactNode;
}

export function FormField({ label, htmlFor, hint, error, children }: Props) {
  const errorId = `${htmlFor}-error`;
  return (
    <div className="field">
      <label htmlFor={htmlFor}>{label}</label>
      {children}
      {hint && <small>{hint}</small>}
      {error && (
        <small id={errorId} role="alert" className="field-error">
          {error}
        </small>
      )}
    </div>
  );
}
