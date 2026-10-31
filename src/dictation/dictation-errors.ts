export type MicPermissionError =
  | "permission-denied"
  | "no-device"
  | "device-in-use"
  | "unsupported"
  | "unknown";

export function classifyGetUserMediaError(error: unknown): MicPermissionError {
  const name = error instanceof DOMException ? error.name : "";
  if (name === "NotAllowedError" || name === "SecurityError") return "permission-denied";
  if (name === "NotFoundError" || name === "DevicesNotFoundError") return "no-device";
  if (name === "NotReadableError" || name === "TrackStartError") return "device-in-use";
  if (name === "NotSupportedError" || name === "TypeError") return "unsupported";
  return "unknown";
}

export async function probeMicrophone(): Promise<{
  stream: MediaStream | null;
  error: MicPermissionError | null;
}> {
  if (!navigator.mediaDevices?.getUserMedia) {
    return { stream: null, error: "unsupported" };
  }
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    return { stream, error: null };
  } catch (error) {
    return { stream: null, error: classifyGetUserMediaError(error) };
  }
}

export async function listAudioInputs(): Promise<MediaDeviceInfo[]> {
  if (!navigator.mediaDevices?.enumerateDevices) return [];
  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices.filter((item) => item.kind === "audioinput");
}
