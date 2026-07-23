import { atom } from "jotai";
import type { FeedbackMode } from "./rewrite-types";

/** Whether the in-browser prose-rewrite edit mode is active (⌥E toggles it). */
export const rewriteModeAtom = atom(false);

/** Default feedback style for new requests: single diff, or N candidates. */
export const feedbackModeAtom = atom<FeedbackMode>("diff");
