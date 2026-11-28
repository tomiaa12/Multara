import { shallowRef } from "vue";

export interface OpenSessionRawDataOptions {
  sessionId: string;
  title?: string;
}

export const sessionRawPayload = shallowRef<OpenSessionRawDataOptions | null>(null);

export function openSessionRawData(options: OpenSessionRawDataOptions): void {
  sessionRawPayload.value = { sessionId: options.sessionId, title: options.title };
}

export function closeSessionRawData(): void {
  sessionRawPayload.value = null;
}
