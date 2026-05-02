import { useEffect, useRef } from 'react';

/**
 * Reproduce un archivo de audio local (ruta absoluta) sincronizado con
 * el estado isPlaying del preview de animaciones. Usa HTMLAudioElement
 * directamente con el esquema file:// — sin I/O adicional ni decode en Rust.
 */
export function useAudioPreview(
  audioPath: string | undefined,
  isPlaying: boolean,
  isLooping: boolean,
): void {
  const audioRef = useRef<HTMLAudioElement | null>(null);

  // Crear/destruir el elemento al cambiar la ruta
  useEffect(() => {
    if (!audioPath) {
      audioRef.current = null;
      return;
    }
    const audio = new Audio(`file:///${audioPath.replace(/\\/g, '/')}`);
    audioRef.current = audio;
    return () => {
      audio.pause();
      audioRef.current = null;
    };
  }, [audioPath]);

  // Sincronizar reproducción con isPlaying
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    if (isPlaying) {
      audio.loop = isLooping;
      audio.currentTime = 0;
      audio.play().catch(() => {
        // Silenciar error si el archivo no existe o el formato no es soportado
      });
    } else {
      audio.pause();
      audio.currentTime = 0;
    }
  }, [isPlaying, isLooping]);

  // Detener al desmontar
  useEffect(() => {
    return () => {
      audioRef.current?.pause();
    };
  }, []);
}
