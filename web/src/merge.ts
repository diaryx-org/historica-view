// How a merge's sides came together: each line of the automatic merge,
// coloured by the parent whose side wrote it, with the places they met.

import { type App, href } from "./app";
import { element, hunksTable, link, short, title } from "./dom";
import { PALETTE } from "./history";
import type { ContestSummary, MergeDetail, MergeFile, RevisionSummary } from "./types";

/// Shared lines kept either side of a side's own, before the rest fold away.
const CONTEXT = 3;

const MEETING: Record<ContestSummary["kind"], string> = {
  insertion: "Both sides wrote here at once; the order between them was chosen by digest.",
  deletion: "One side removed lines that another wrote beside.",
  terminator: "The sides disagree about the last line's newline.",
};

function sideColour(side: number): string {
  return PALETTE[side % PALETTE.length];
}

function legend(app: App, merge: MergeDetail): HTMLElement {
  const to = (id: string) => link(short(id), href({ view: "history", id }), () => app.go({ view: "history", id }));
  return element(
    "ul",
    { class: "sides" },
    ...merge.parents.map((parent, index) =>
      element(
        "li",
        {},
        element("span", { class: "swatch", style: `background:${sideColour(index)}` }),
        `Parent ${index + 1} `,
        to(parent),
        " ",
        element("span", { class: "quiet" }, title(app.byId.get(parent)?.message ?? "")),
      ),
    ),
    element("li", {}, element("span", { class: "swatch shared" }), "Lines both sides already had"),
  );
}

/// The lines worth showing: every side's own and contested line, with
/// `CONTEXT` shared lines around them.
function shown(file: MergeFile, contested: Set<number>): boolean[] {
  const near = file.lines.map((line, index) => line.side !== null || contested.has(index));
  for (const contest of file.contests) if (contest.len === 0) near[Math.min(contest.at, near.length - 1)] = true;
  return near.map((_, index) => {
    for (let at = Math.max(0, index - CONTEXT); at <= Math.min(near.length - 1, index + CONTEXT); at++) {
      if (near[at]) return true;
    }
    return false;
  });
}

function linesTable(app: App, file: MergeFile): HTMLElement {
  const contested = new Set<number>();
  const starts = new Map<number, ContestSummary[]>();
  for (const contest of file.contests) {
    for (let at = contest.at; at < contest.at + contest.len; at++) contested.add(at);
    starts.set(contest.at, [...(starts.get(contest.at) ?? []), contest]);
  }
  const visible = shown(file, contested);
  const table = element("table", { class: "diff merged" });
  let folded = 0;
  const fold = () => {
    if (!folded) return;
    table.append(element("tr", { class: "hunk" }, element("td", { colspan: "3" }, `⋯ ${folded} shared line${folded === 1 ? "" : "s"}`)));
    folded = 0;
  };
  const meeting = (contest: ContestSummary) =>
    element(
      "tr",
      { class: "meeting" },
      element("td", { colspan: "3" }, `⚑ ${MEETING[contest.kind]}`),
    );

  let previous: string | null = null;
  file.lines.forEach((line, index) => {
    if (!visible[index]) {
      folded++;
      previous = null;
      return;
    }
    fold();
    for (const contest of starts.get(index) ?? []) table.append(meeting(contest));
    const origin =
      line.origin === previous
        ? ""
        : link(short(line.origin), href({ view: "history", id: line.origin }), () => app.go({ view: "history", id: line.origin }));
    previous = line.origin;
    const bar = line.side === null ? "" : `box-shadow: inset 4px 0 ${sideColour(line.side)}`;
    table.append(
      element(
        "tr",
        { class: contested.has(index) ? "contested" : "", title: title(app.byId.get(line.origin)?.message ?? "") },
        element("td", { class: "number", style: bar }, String(index + 1)),
        element("td", { class: "origin" }, origin),
        element("td", { class: `text${line.side === null ? " shared" : ""}` }, line.text),
      ),
    );
  });
  fold();
  for (const contest of starts.get(file.lines.length) ?? []) table.append(meeting(contest));
  return table;
}

function fileSection(app: App, merge: MergeDetail, file: MergeFile): HTMLElement {
  const marks: (Node | string)[] = [];
  const met = file.contests.filter((contest) => contest.kind !== "terminator").length;
  marks.push(
    element("span", { class: `kind ${met ? "dropped" : "added"}` }, met ? `met in ${met} place${met === 1 ? "" : "s"}` : "merged cleanly"),
  );
  if (file.hand) marks.push(" ", element("span", { class: "kind edited" }, "settled by hand"));
  const open = { view: "files" as const, at: merge.id, file: file.file };
  const body: HTMLElement[] = [linesTable(app, file)];
  if (file.hand) {
    body.push(
      element("p", { class: "quiet" }, "What the merge recorded in place of the automatic merge:"),
      hunksTable(file.hand),
    );
  }
  return element(
    "details",
    { open: "" },
    element("summary", {}, ...marks, " ", file.path, " ", link("history of this file", href(open), () => app.go(open))),
    ...body,
  );
}

export function mergeView(app: App, revision: RevisionSummary): HTMLElement[] {
  const merge = JSON.parse(app.viewer.merge(revision.id)) as MergeDetail;
  const intro = merge.files.length
    ? element("p", { class: "quiet" }, "Every file both sides changed, as the library merges them, each line marked with the side that wrote it.")
    : element(
        "p",
        { class: "quiet" },
        "No file was changed on more than one side: the merge took each side's files whole. Compare against a parent to see what came in.",
      );
  return [legend(app, merge), intro, element("div", { class: "files" }, ...merge.files.map((file) => fileSection(app, merge, file)))];
}
