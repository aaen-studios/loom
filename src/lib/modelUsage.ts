/**
 * Where a model is referenced.
 *
 * Unselecting a model hides it from the pickers but deliberately does *not*
 * break anything already pointing at it — the app default, a chat, the lite
 * model, the image or embedding model. That is the right behaviour, but it is
 * invisible: the row quietly disappears while four settings keep using it. This
 * module is the one place that answers "who still needs this?", so the "in use"
 * chip and the warning text in Settings → Providers cannot drift apart.
 */

import type { AuxModelRef, ModelRef } from "../types";

/** The parts of `chat` defaults that can point at a model. */
export interface ChatModelRefs {
  providerId: string | null;
  modelId: string | null;
  lite: ModelRef | null;
  imageModel: AuxModelRef | null;
  embeddingModel: AuxModelRef | null;
  computerModel: ModelRef | null;
  recentModels: ModelRef[];
}

/** Just enough of a provider map to answer "who serves this id?". */
export type ProviderIndex = Record<
  string,
  { name: string; models: Record<string, unknown> } | undefined
>;

/**
 * Whether an auxiliary ref points at this exact model.
 *
 * An unqualified ref (no `providerId`) matches the id in *any* provider, which
 * is what a legacy bare string means — so two providers serving the same id are
 * both truthfully "in use", and the ambiguity is reported separately rather
 * than one of them being hidden.
 */
export function auxRefMatches(
  ref: AuxModelRef | null | undefined,
  providerId: string,
  modelId: string,
): boolean {
  if (!ref) return false;
  if (ref.modelId !== modelId) return false;
  const qualified = (ref.providerId ?? "").trim();
  return qualified === "" || qualified === providerId;
}

/** Ids of every provider serving a model id, in id order. */
export function providersServing(
  providers: ProviderIndex,
  modelId: string,
): string[] {
  if (!modelId) return [];
  return Object.keys(providers)
    .filter((id) => modelId in (providers[id]?.models ?? {}))
    .sort();
}

/** A provider's display name, falling back to its id. */
export function providerLabel(providers: ProviderIndex, id: string): string {
  return providers[id]?.name ?? id;
}

/** Human label for an aux ref: `Provider · model`, or just the model id. */
export function auxRefLabel(
  providers: ProviderIndex,
  ref: AuxModelRef | null | undefined,
): string {
  if (!ref) return "";
  const qualified = (ref.providerId ?? "").trim();
  return qualified
    ? `${providerLabel(providers, qualified)} · ${ref.modelId}`
    : ref.modelId;
}

/**
 * Everywhere this model is still referenced, as short labels.
 *
 * Order is stable (app default, lite, image, embedding, computer) so the chip
 * does not reshuffle as unrelated config changes.
 */
export function referencesTo(
  chat: ChatModelRefs,
  providerId: string,
  modelId: string,
): string[] {
  const labels: string[] = [];
  const isDefault =
    chat.providerId === providerId && chat.modelId === modelId;
  if (isDefault) labels.push("App default");
  if (
    chat.lite?.providerId === providerId &&
    chat.lite?.modelId === modelId
  ) {
    labels.push("Lite model");
  }
  if (auxRefMatches(chat.imageModel, providerId, modelId)) {
    labels.push("Image model");
  }
  if (auxRefMatches(chat.embeddingModel, providerId, modelId)) {
    labels.push("Embedding model");
  }
  if (
    chat.computerModel?.providerId === providerId &&
    chat.computerModel?.modelId === modelId
  ) {
    labels.push("Computer model");
  }
  return labels;
}

/** Whether the model appears in the picker's recents list. */
export function isRecent(
  chat: ChatModelRefs,
  providerId: string,
  modelId: string,
): boolean {
  return chat.recentModels.some(
    (entry) => entry.providerId === providerId && entry.modelId === modelId,
  );
}

/**
 * A warning when an unqualified aux ref is served by more than one provider.
 *
 * Two plans serving one embedding id make a bare string genuinely ambiguous,
 * and the engine resolves it by a fixed precedence. Saying which providers
 * *could* serve it — rather than pretending the choice is obvious — is the
 * honest report; the settings row shows the id so the user can pin it.
 */
export function auxAmbiguity(
  providers: ProviderIndex,
  ref: AuxModelRef | null | undefined,
  field: string,
): string | null {
  if (!ref) return null;
  const qualified = (ref.providerId ?? "").trim();
  if (qualified) return null;
  const candidates = providersServing(providers, ref.modelId);
  if (candidates.length < 2) return null;
  const names = candidates.map((id) => providerLabel(providers, id)).join(", ");
  return `${field} is set to “${ref.modelId}”, which ${candidates.length} providers serve (${names}). Loom uses one of them — pin the provider to be sure which.`;
}

/** How many models a provider serves. */
export function totalModels(
  providers: ProviderIndex,
  providerId: string,
): number {
  return Object.keys(providers[providerId]?.models ?? {}).length;
}

/**
 * How many instances share a provider's preset, and which one this is.
 *
 * Used to label two cards built from one preset ("OpenCode Go (2 of 2)") so two
 * plans are distinguishable without opening either.
 */
export function instancePosition(
  providers: ProviderIndex,
  presetOf: (id: string) => string | null,
  id: string,
): { count: number; index: number } {
  const mine = presetOf(id);
  if (!mine) return { count: 1, index: 1 };
  const siblings = Object.keys(providers)
    .filter((other) => presetOf(other) === mine)
    .sort();
  return { count: siblings.length, index: siblings.indexOf(id) + 1 };
}
