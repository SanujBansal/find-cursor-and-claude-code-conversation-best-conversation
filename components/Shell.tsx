import { Sidebar } from "./Sidebar";

export function Shell({
  title,
  subtitle,
  children,
  actions,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  actions?: React.ReactNode;
}) {
  return (
    <div className="flex h-screen">
      <Sidebar />
      <main className="flex h-screen flex-1 flex-col overflow-hidden">
        <header className="flex items-center justify-between border-b border-panel-border bg-panel px-6 py-4">
          <div>
            <h2 className="text-lg font-semibold text-foreground">{title}</h2>
            {subtitle ? (
              <p className="mt-0.5 text-sm text-muted">{subtitle}</p>
            ) : null}
          </div>
          {actions && <div className="flex items-center gap-2">{actions}</div>}
        </header>
        <div className="flex-1 overflow-y-auto p-6">{children}</div>
      </main>
    </div>
  );
}
