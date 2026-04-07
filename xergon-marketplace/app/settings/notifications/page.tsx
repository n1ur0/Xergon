"use client";

import { NotificationPreferences } from "@/components/settings/NotificationPreferences";
import { EmailDigestSettings } from "@/components/settings/EmailDigestSettings";

export default function NotificationsPage() {
  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-surface-900">Notifications</h2>
        <p className="text-sm text-surface-800/50 mt-0.5">Configure how and when you receive notifications</p>
      </div>
      <NotificationPreferences />
      <div className="border-t border-surface-200 pt-8">
        <EmailDigestSettings />
      </div>
    </div>
  );
}
