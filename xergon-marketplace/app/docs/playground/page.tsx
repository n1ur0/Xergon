import { Metadata } from "next";
import ApiPlayground from "@/components/docs/ApiPlayground";

export const metadata: Metadata = {
  title: "API Playground | Xergon",
  description: "Interactive API playground for testing Xergon Relay API endpoints.",
};

export default function PlaygroundPage() {
  return <ApiPlayground />;
}
