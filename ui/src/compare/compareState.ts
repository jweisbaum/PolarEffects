/**
 * What the Compare stage is comparing (spec.md 11): operands A and B, Δ in
 * knots or percent of B, and the threshold that splits "A faster" from "B
 * faster". View state, like the source being edited: never saved, never in
 * the history, but kept in memory per project while the app runs, so
 * leaving the stage and coming back finds the same comparison.
 */
import { useSyncExternalStore } from "react";

import type { CompareOperand } from "../generated/CompareOperand";
import type { ProjectSummary } from "../generated/ProjectSummary";

/** The default threshold, knots (spec.md 11). */
export const DEFAULT_THRESHOLD_KN = 0.05;

export interface CompareChoice {
  a: CompareOperand;
  b: CompareOperand;
  /** Δ as percent of B rather than in speed. */
  percent: boolean;
  /** Knots. */
  thresholdKn: number;
}

const BLEND: CompareOperand = { kind: "blend" };

/** Per project id; entries live as long as the app. */
const choices = new Map<number, CompareChoice>();
const listeners = new Set<() => void>();
let version = 0;

function notify() {
  version += 1;
  for (const listener of listeners) listener();
}

/** The operand that names `source`: its segment for a track, its polar otherwise. */
export function operandOf(source: { id: number; kind: string }): CompareOperand {
  return source.kind === "track" ? { kind: "segment", source_id: source.id } : { kind: "polar", source_id: source.id };
}

/** Whether an operand still names something in the project. */
export function operandExists(project: ProjectSummary, operand: CompareOperand): boolean {
  if (operand.kind === "blend") return true;
  return project.sources.some((s) => s.id === operand.source_id && (s.kind === "track") === (operand.kind === "segment"));
}

/**
 * The default comparison: the first source against the blend, or the blend
 * against itself in an empty project.
 */
export function defaultChoice(project: ProjectSummary): CompareChoice {
  const first = project.sources[0];
  return { a: first ? operandOf(first) : BLEND, b: BLEND, percent: false, thresholdKn: DEFAULT_THRESHOLD_KN };
}

/**
 * The project's comparison. An operand whose source was removed falls back
 * to the default's (the view never asks Rust for a source that is gone).
 */
export function compareChoice(project: ProjectSummary): CompareChoice {
  const held = choices.get(project.id);
  const fallback = defaultChoice(project);
  if (!held) return fallback;
  return {
    ...held,
    a: operandExists(project, held.a) ? held.a : fallback.a,
    b: operandExists(project, held.b) ? held.b : BLEND,
  };
}

/** Changes part of a project's comparison. */
export function setCompareChoice(project: ProjectSummary, change: Partial<CompareChoice>): void {
  choices.set(project.id, { ...compareChoice(project), ...change });
  notify();
}

/** Swaps A and B. */
export function swapOperands(project: ProjectSummary): void {
  const current = compareChoice(project);
  setCompareChoice(project, { a: current.b, b: current.a });
}

const openListeners = new Set<() => void>();

/**
 * A source row's Compare (spec.md 8): the source as A, the blend as B, and
 * the Compare stage brought forward.
 */
export function compareSource(project: ProjectSummary, sourceId: number): void {
  const source = project.sources.find((s) => s.id === sourceId);
  if (!source) return;
  setCompareChoice(project, { a: operandOf(source), b: BLEND });
  for (const listener of openListeners) listener();
}

/** Runs `listener` whenever a source asks to be compared; returns the unsubscribe. */
export function onCompareSource(listener: () => void): () => void {
  openListeners.add(listener);
  return () => openListeners.delete(listener);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The project's comparison, re-rendering on every change. */
export function useCompareChoice(project: ProjectSummary): CompareChoice {
  useSyncExternalStore(subscribe, () => version);
  return compareChoice(project);
}

/** Forgets every comparison (tests). */
export function resetCompareChoices(): void {
  choices.clear();
  notify();
}
