// Port of BetterWispr's VocabularyView: names and terms OpenDictate should spell your way.
import { ArrowRight, BookText, Plus, Sparkles, Trash2, Type } from "lucide-react";
import { useState, type FormEvent } from "react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import { Badge, Button, Field, PageHeader, Row, Section, Tile, Toggle } from "../components/ui";
import "./Vocabulary.css";

const MAX_LENGTH = 100;
const MAX_ENTRIES = 200;

export function Vocabulary() {
  const { workspace, run, saveSettings } = useStore();
  const { vocabulary, settings } = workspace;
  const [phrase, setPhrase] = useState("");
  const [replacement, setReplacement] = useState("");
  const full = vocabulary.length >= MAX_ENTRIES;
  const duplicate = vocabulary.some((e) => e.phrase.toLowerCase() === phrase.trim().toLowerCase());
  const canAdd = phrase.trim() !== "" && !full && !duplicate;

  async function add(e: FormEvent) {
    e.preventDefault();
    if (!canAdd) return;
    if (await run(() => api.addVocabulary(phrase.trim(), replacement.trim()))) {
      setPhrase("");
      setReplacement("");
    }
  }

  return (
    <>
      <PageHeader
        title="Vocabulary"
        subtitle="Teach OpenDictate the names, terms and phrases you use, so they're spelled right."
      />

      <section className="section">
        <div className="section-title">
          <h2>Add a word</h2>
        </div>
        <form className="card card-pad stack" onSubmit={add}>
          <div className="grid-2">
            <Field label="Spoken phrase">
              <input
                className="input"
                value={phrase}
                maxLength={MAX_LENGTH}
                placeholder="open dictate"
                onChange={(e) => setPhrase(e.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
            </Field>
            <Field label="Write as">
              <input
                className="input"
                value={replacement}
                maxLength={MAX_LENGTH}
                placeholder="Optional, e.g. OpenDictate"
                onChange={(e) => setReplacement(e.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
            </Field>
          </div>
          <div className="hstack" style={{ alignItems: "flex-start", gap: 20 }}>
            <span className="faint" style={{ flex: 1 }}>
              {full
                ? `Your vocabulary has ${MAX_ENTRIES} entries, the most it can hold. Remove one to add another.`
                : duplicate
                  ? "That phrase is already in your vocabulary."
                  : "Leave “Write as” empty to add a spelling hint. Replacements apply to whole phrases, so unrelated words stay intact."}
            </span>
            <Button type="submit" variant="primary" icon={Plus} disabled={!canAdd}>
              Add word
            </Button>
          </div>
        </form>
      </section>

      <Section title="How it works">
        <Row
          icon={<Tile icon={Sparkles} color="#22c55e" />}
          title="Spelling hints"
          detail="Every entry is given to Whisper models as a hint before they transcribe, so names and terms come out spelled your way. Parakeet models don't take hints, so for them add a “Write as” spelling."
        />
        <Row
          icon={<Tile icon={Type} color="#0a84ff" />}
          title="Replacements"
          detail="When an entry has a “Write as” spelling, the spoken phrase is swapped for it after transcription, with any model. Only whole phrases are replaced."
        />
      </Section>

      <Section
        title={vocabulary.length ? `${vocabulary.length} ${vocabulary.length === 1 ? "entry" : "entries"}` : "Your vocabulary"}
        action={vocabulary.length > 0 ? <span className="faint">{vocabulary.length} of {MAX_ENTRIES}</span> : undefined}
        footer={
          settings.learnCorrections && vocabulary.length > 0
            ? "Entries marked Learned come from fixes you made to a dictation, in History or right after OpenDictate pasted it. Remove any you don't want."
            : undefined
        }
      >
        {vocabulary.length === 0 ? (
          <div className="vocab-empty">
            <BookText size={20} aria-hidden="true" />
            Add a name that often gets misspelled, a technical term or a phrase you use every day.
          </div>
        ) : (
          vocabulary.map((entry) => (
            <div className="row vocab-row" key={entry.id}>
              <div className="row-label vocab-phrase">
                <span className="selectable">{entry.phrase}</span>
                {entry.learned && (
                  <span title="Added from a correction you made">
                    <Badge tone="accent">Learned</Badge>
                  </span>
                )}
              </div>
              <div className="row-control">
                {entry.replacement ? (
                  <span className="vocab-replacement selectable">
                    <ArrowRight size={14} aria-label="written as" />
                    {entry.replacement}
                  </span>
                ) : (
                  <span className="faint">Spelling hint</span>
                )}
                <Button
                  variant="ghost"
                  size="icon"
                  icon={Trash2}
                  title={`Remove ${entry.phrase}`}
                  onClick={() => run(() => api.deleteVocabulary(entry.id))}
                />
              </div>
            </div>
          ))
        )}
      </Section>

      <Section title="Learning">
        <Row
          title="Learn from my corrections"
          detail="When you fix a misheard word in History, or in the text field within 30 seconds of a paste, it is added to Vocabulary."
        >
          <Toggle
            label="Learn from my corrections"
            on={settings.learnCorrections}
            onChange={(on) => saveSettings((s) => ({ ...s, learnCorrections: on }))}
          />
        </Row>
      </Section>
    </>
  );
}
