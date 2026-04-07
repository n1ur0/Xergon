import "@/styles/mobile.css";
import { MobileNav } from "@/components/layout/MobileNav";

export default function MobileLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="mobile-layout">
      <MobileNav />
      <div className="mobile-content pb-20">{children}</div>
    </div>
  );
}
