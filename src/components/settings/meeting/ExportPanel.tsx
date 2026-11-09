import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { commands } from "@/bindings";
import { notify } from "@/stores/noticeStore";
import { Button } from "../../ui/Button";
import { SettingsGroup } from "../../ui/SettingsGroup";

interface ExportPanelProps {
  dir: string | null;
  onChanged: (dir: string | null) => void;
}

/** Where meetings are written as Markdown notes, such as an Obsidian vault. */
export const ExportPanel: React.FC<ExportPanelProps> = ({ dir, onChanged }) => {
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);

  const save = async (next: string | null) => {
    const result = await commands.setMeetingExportDir(next);
    if (result.status === "error") {
      notify({
        title: t("settings.meeting.export.setFailed", { error: result.error }),
      });
      return;
    }
    onChanged(next);
  };

  const choose = async () => {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        defaultPath: dir ?? undefined,
        title: t("settings.meeting.export.choose"),
      });
      if (typeof picked === "string") await save(picked);
    } catch (error) {
      console.error("Failed to pick the export folder:", error);
    }
  };

  const exportAll = async () => {
    setExporting(true);
    const result = await commands.exportAllMeetings();
    setExporting(false);
    notify({
      title:
        result.status === "ok"
          ? t("settings.meeting.export.allDone", { count: result.data })
          : t("settings.meeting.export.failed", { error: result.error }),
    });
  };

  return (
    <SettingsGroup
      title={t("settings.meeting.export.title")}
      description={t("settings.meeting.export.description")}
    >
      <div className="px-4 py-3 flex items-center gap-3">
        <FolderOpen className="w-4 h-4 shrink-0 text-ink-3" />
        <p
          className={`flex-1 min-w-0 truncate text-sm ${dir ? "font-mono" : "text-ink-2"}`}
          title={dir ?? undefined}
        >
          {dir ?? t("settings.meeting.export.noFolder")}
        </p>
        <Button variant="secondary" size="sm" onClick={choose}>
          {dir
            ? t("settings.meeting.export.change")
            : t("settings.meeting.export.choose")}
        </Button>
        {dir && (
          <Button variant="ghost" size="sm" onClick={() => save(null)}>
            {t("settings.meeting.export.stop")}
          </Button>
        )}
      </div>
      {dir && (
        <div className="px-4 py-3 flex items-center justify-between gap-4">
          <p className="text-xs text-ink-3">
            {t("settings.meeting.export.auto")}
          </p>
          <Button
            variant="secondary"
            size="sm"
            onClick={exportAll}
            disabled={exporting}
          >
            {exporting
              ? t("settings.meeting.export.exporting")
              : t("settings.meeting.export.all")}
          </Button>
        </div>
      )}
    </SettingsGroup>
  );
};
