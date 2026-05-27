export function PlaceholderPanel({
  label,
  description,
}: {
  label: string;
  description: string;
}) {
  return (
    <section className="rounded border border-panel-border bg-panel p-5">
      <p className="text-xs uppercase tracking-[0.16em] text-accent">{label}</p>
      <p className="mt-3 text-sm leading-relaxed text-muted">{description}</p>
    </section>
  );
}
