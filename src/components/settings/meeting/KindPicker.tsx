import React from "react";
import { useTranslation } from "react-i18next";
import { MonitorSpeaker, Users } from "lucide-react";
import type { MeetingKind } from "@/bindings";

interface KindPickerProps {
  value: MeetingKind;
  onChange: (kind: MeetingKind) => void;
  disabled?: boolean;
}

/** Online (a call) or on site (one room), side by side. */
export const KindPicker: React.FC<KindPickerProps> = ({
  value,
  onChange,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const kinds: { kind: MeetingKind; label: string; icon: typeof Users }[] = [
    {
      kind: "online",
      label: t("settings.meeting.kindOnline"),
      icon: MonitorSpeaker,
    },
    { kind: "on_site", label: t("settings.meeting.kindOnSite"), icon: Users },
  ];
  return (
    <div
      role="radiogroup"
      aria-label={t("settings.meeting.fields.kind")}
      className="inline-flex rounded-lg border border-line-strong overflow-hidden"
    >
      {kinds.map(({ kind, label, icon: Icon }) => {
        const selected = value === kind;
        return (
          <button
            key={kind}
            type="button"
            role="radio"
            aria-checked={selected}
            disabled={disabled}
            onClick={() => onChange(kind)}
            className={`flex items-center gap-2 px-3 py-1.5 text-sm transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
              selected
                ? "bg-logo-primary/20 text-text font-medium"
                : "bg-wash text-ink-2 hover:bg-logo-primary/10"
            }`}
          >
            <Icon className="w-4 h-4" />
            {label}
          </button>
        );
      })}
    </div>
  );
};
