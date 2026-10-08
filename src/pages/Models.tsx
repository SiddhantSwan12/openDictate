// Port of BetterWispr's ModelsView: speech models, optional API connections and the notes model.
import { BookText, Download } from "lucide-react";
import { Banner, Button, PageHeader, Row, Section, Tile } from "../components/ui";
import { useStore } from "../lib/store";
import { LocalModels } from "./models/LocalModels";
import { NotesModelSettings } from "./models/NotesModelSettings";
import { SpeechConnections } from "./models/SpeechConnections";
import "./models/Models.css";

export function Models() {
  const { workspace, session, setPage } = useStore();
  const selected = workspace.models.find((m) => m.id === workspace.settings.selectedModelID);
  const needsDownload = selected && !selected.installed && session.installation?.id !== selected.id;

  return (
    <>
      <PageHeader
        title="Models"
        subtitle="Choose how OpenDictate turns speech into text and who writes your meeting notes. Built-in models are free, unlimited and run on this PC."
      />

      {needsDownload && (
        <Banner tone="info">
          <span className="models-inline">
            <Download size={14} />
            Download {selected.name} below to start dictating, or choose another model.
          </span>
        </Banner>
      )}

      <LocalModels />
      <SpeechConnections />
      <NotesModelSettings />

      <Section>
        <Row
          icon={<Tile icon={BookText} color="#22c55e" size={32} />}
          title="Improve accuracy"
          detail="Larger models handle accents and challenging audio better, but use more memory and take longer. A quiet microphone, the correct language and your personal vocabulary also make a difference."
        >
          <Button onClick={() => setPage("vocabulary")}>Open Vocabulary</Button>
        </Row>
      </Section>
    </>
  );
}
