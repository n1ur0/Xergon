import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Conversation | Xergon Marketplace",
  description: "View your conversation with a provider on Xergon Marketplace.",
};

export default function ThreadPage() {
  return <ThreadClient />;
}

import { ThreadClient } from "./ThreadClient";
