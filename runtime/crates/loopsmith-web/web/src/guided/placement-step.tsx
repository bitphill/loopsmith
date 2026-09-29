/**
 * Where the loop lands, and in which grammar.
 *
 * The loop is created in a directory of its own, named after it, inside the
 * folder picked here. Creating straight into the picked folder is the trap the
 * expert editor already avoids: the obvious thing to pick is something like
 * `~/loops`, and a loop scaffolded directly into that turns the container into
 * the loop.
 */
import { Field, Note, Select, Text, Toggle } from "../ui";
import type { Format, PathFacts } from "../types";

export function PlacementStepBody({
  parent, setParent, loopPath, initGit, setInitGit, format, setFormat, facts,
}: {
  parent: string;
  setParent: (v: string) => void;
  loopPath: string;
  initGit: boolean;
  setInitGit: (v: boolean) => void;
  format: Format;
  setFormat: (f: Format) => void;
  facts: PathFacts | null;
}) {
  return (
    <div className="space-y-3">
      <Field label="Folder to create it in" hint="The loop nests in a sub-directory named after it.">
        {(id) => (
          <Text id={id} value={parent} onChange={setParent} placeholder="~/loops" mono />
        )}
      </Field>

      {loopPath && (
        <p className="hint">
          Creating at <span className="font-mono text-text">{loopPath}</span>
        </p>
      )}

      {facts && !facts.writable && (
        <Note tone="error">That folder is not writable.</Note>
      )}
      {facts?.existing_loop && (
        <Note tone="warning">A loop called {facts.existing_loop} already lives there.</Note>
      )}

      <Field label="Config grammar" hint="Markdown reads like a brief; YAML is terser and closer to the schema.">
        {(id) => (
          <Select
            id={id}
            value={format}
            onChange={(v) => setFormat(v as Format)}
            options={[
              { value: "markdown", label: "Markdown — easiest to hand-edit later" },
              { value: "yaml", label: "YAML — closer to the schema" },
            ]}
          />
        )}
      </Field>

      <Toggle
        checked={initGit}
        onChange={setInitGit}
        label="Initialise a git repository"
        hint="Lets isolated nodes get a worktree each. Safe to say yes."
      />
    </div>
  );
}
