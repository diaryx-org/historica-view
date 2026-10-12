// What the view module answers, as the page reads it.

export interface RevisionSummary {
  id: string;
  change: string;
  parents: string[];
  supersedes: string[];
  author: string;
  when: string;
  epoch: number;
  message: string;
  head: boolean;
  superseded: boolean;
  divergent: boolean;
}

export interface NameSummary {
  name: string;
  kind: "revision" | "change" | "file";
  target: string;
  private: boolean;
  revision: string | null;
}

export interface Graph {
  revisions: RevisionSummary[];
  names: NameSummary[];
}

export interface Line {
  sign: " " | "-" | "+";
  text: string;
}

export interface Hunk {
  old_start: number;
  new_start: number;
  lines: Line[];
}

export interface FileChange {
  file: string;
  from: string | null;
  to: string | null;
  kind: "added" | "dropped" | "edited" | "moved" | "mode";
  hunks: Hunk[] | null;
  mode: [string, string] | null;
}

export interface RevisionDetail {
  id: string;
  against: string | null;
  files: FileChange[];
}

export interface TreeFile {
  file: string;
  path: string;
  kind: "lines" | "bytes" | "link";
  size: number | null;
}

export interface Version {
  revision: string;
  kind: "added" | "edited" | "moved" | "mode" | "dropped";
  path: string | null;
}

export interface FolderVersion {
  revision: string;
  files: number;
}

export interface FileAt {
  file: string;
  path: string;
  kind: "lines" | "bytes" | "link";
  text: string | null;
  size: number | null;
  target: string | null;
  held: boolean;
}
