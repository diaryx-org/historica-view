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

export interface MergeLine {
  text: string;
  origin: string;
  /// The parent whose side alone wrote the line, or null for one they shared.
  side: number | null;
}

export interface ContestSummary {
  at: number;
  len: number;
  kind: "insertion" | "deletion" | "terminator";
  revisions: string[];
}

export interface MergeFile {
  file: string;
  path: string;
  lines: MergeLine[];
  contests: ContestSummary[];
  hand: Hunk[] | null;
}

export interface MergeDetail {
  id: string;
  parents: string[];
  files: MergeFile[];
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

export interface Approval {
  /// The minisign public key that made the claim.
  key: string;
  role: string;
  when: string;
  /// The one file a claim over one file covers, or null for the whole tip.
  file: string | null;
  /// Whether the signature verifies under the key the claim names.
  verified: boolean;
  /// Who this copy's own trust takes the key to speak for, if it holds it.
  who: string | null;
}

export interface Proposal {
  /// The bookmark, `changes/<id>`.
  name: string;
  title: string;
  tip: string;
  /// The revisions the tip reaches and the accepted line does not, newest
  /// first.
  revisions: string[];
  approvals: Approval[];
}

export interface Task {
  file: string;
  path: string;
  title: string;
  status: string;
  closed: boolean;
  created: string | null;
  updated: string | null;
  author: string | null;
  description: string | null;
}

export interface TaskDetail extends Task {
  /// Unsanitised: the page sanitises before it shows it.
  html: string;
}

export interface Overview {
  title: string | null;
  description: string | null;
  readme: { file: string; path: string; html: string } | null;
  open_tasks: number;
  closed_tasks: number;
}

export interface Listing {
  header: string;
  files: { path: string; store: string; digest: string }[];
}
