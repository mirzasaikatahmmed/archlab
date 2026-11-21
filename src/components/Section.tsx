import type { ReactNode } from "react";

interface Props {
  title: string;
  description?: string;
  children: ReactNode;
}

/** A titled group of fields: the title explains the group, the fields sit beside it. */
export function Section({ title, description, children }: Props) {
  return (
    <section className="section">
      <div className="section-head">
        <h2>{title}</h2>
        {description && <p>{description}</p>}
      </div>
      <div className="section-body">{children}</div>
    </section>
  );
}
