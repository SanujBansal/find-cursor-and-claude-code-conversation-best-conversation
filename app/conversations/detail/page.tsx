"use client";

import { Suspense } from "react";
import { ConversationDetail } from "./ConversationDetail";

export default function ConversationDetailPage() {
  return (
    <Suspense fallback={<div className="flex min-h-screen items-center justify-center text-sm text-muted">Loading…</div>}>
      <ConversationDetail />
    </Suspense>
  );
}
