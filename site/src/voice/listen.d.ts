export const MODEL: string
export function canListen(): boolean
export function askMic(): Promise<MediaStream>
export function recognizer(prefer?: string | null): {
  worker: Worker
  listen(fn: (m: any) => void): () => void
  load(): Promise<any>
}
export function listen(options: { context: AudioContext; stream: MediaStream; asr: ReturnType<typeof recognizer>; onLevel?(v: number): void }): Promise<{ timeOf(sample: number): number; stop(): void }>
