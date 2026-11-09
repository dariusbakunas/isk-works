/** What the Stages inspector shows: one production node or one acquisition line, by stable identity. */
export type StagesSelection =
  | { kind: "production"; nodeId: string }
  | { kind: "acquisition"; typeId: number };
