import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Messages | Xergon Marketplace",
  description: "Direct messaging with compute providers on Xergon Marketplace.",
};

export default function MessagesPage() {
  return (
    <div className="h-[calc(100vh-4rem)]">
      <MessagesClient />
    </div>
  );
}

import { MessagesClient } from "./MessagesClient";
