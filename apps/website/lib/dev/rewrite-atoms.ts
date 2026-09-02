import { atom } from "jotai";
import type { FeedbackMode } from "./rewrite-types";

export const rewriteModeAtom = atom(false);

export const feedbackModeAtom = atom<FeedbackMode>("diff");
