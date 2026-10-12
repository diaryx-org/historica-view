// Lanes for a revision graph, drawn one row per revision.
//
// The rows arrive newest first with every child above its parents. Walking
// down them, each lane holds the revision it is waiting to reach; a revision
// takes the lane that waited for it (or a free one), its first parent
// continues that lane, and its other parents open lanes of their own. Lanes
// that waited for the same revision close into it.

export interface Row {
  id: string;
  parents: string[];
}

/// One line between a row and the row below it.
export interface Edge {
  /// The lane it leaves this row from.
  from: number;
  /// The lane it reaches the next row in.
  to: number;
  /// The lane whose colour it takes.
  lane: number;
}

export interface Layout {
  /// The lane each row's node sits in.
  columns: number[];
  /// The lines below each row.
  edges: Edge[][];
  /// The most lanes open at once.
  width: number;
}

export function layout(rows: Row[]): Layout {
  const lanes: (string | null)[] = [];
  const columns: number[] = [];
  const below: { lane: number; id: string; fromNode: boolean }[][] = [];
  const present = new Set(rows.map((row) => row.id));
  let width = 0;

  const free = (): number => {
    const index = lanes.indexOf(null);
    if (index >= 0) return index;
    lanes.push(null);
    return lanes.length - 1;
  };

  for (const row of rows) {
    let column = lanes.indexOf(row.id);
    if (column < 0) column = free();
    // Every other lane that waited for this revision closes here.
    lanes.forEach((waiting, index) => {
      if (waiting === row.id && index !== column) lanes[index] = null;
    });
    lanes[column] = null;
    columns.push(column);

    const fromNode = new Set<number>();
    // Parents the store does not hold have nothing to draw a line to.
    const parents = row.parents.filter((parent) => present.has(parent));
    parents.forEach((parent, index) => {
      const already = lanes.indexOf(parent);
      if (already >= 0) {
        // A lane already waits for it: join that lane rather than open another.
        fromNode.add(already);
        return;
      }
      const lane = index === 0 && lanes[column] === null ? column : free();
      lanes[lane] = parent;
      fromNode.add(lane);
    });

    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
    width = Math.max(width, lanes.length, column + 1);
    below.push(
      lanes.flatMap((id, lane) =>
        id === null ? [] : [{ lane, id, fromNode: fromNode.has(lane) }],
      ),
    );
  }

  const edges = below.map((open, index) =>
    open.map(({ lane, id, fromNode }) => {
      const next = rows[index + 1];
      return {
        from: fromNode ? columns[index] : lane,
        to: next && next.id === id ? columns[index + 1] : lane,
        lane,
      };
    }),
  );
  return { columns, edges, width };
}
