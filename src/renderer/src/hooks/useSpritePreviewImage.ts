import { useEffect, useState } from 'react';

interface ImageSize {
  width: number;
  height: number;
}

interface UseSpritePreviewImageResult {
  imageSrc: string;
  imageSize: ImageSize | null;
}

export function useSpritePreviewImage(spritePath: string): UseSpritePreviewImageResult {
  const [imageSrc, setImageSrc] = useState('');
  const [imageSize, setImageSize] = useState<ImageSize | null>(null);

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      setImageSrc('');
      setImageSize(null);
      if (!spritePath) return;

      const dataUrl = await window.electronAPI.getImageDataUrl(spritePath);
      if (cancelled || !dataUrl) return;

      const img = new window.Image();
      img.onload = () => {
        if (cancelled) return;
        setImageSrc(dataUrl);
        setImageSize({ width: img.width, height: img.height });
      };
      img.onerror = () => {
        if (cancelled) return;
        setImageSrc('');
        setImageSize(null);
      };
      img.src = dataUrl;
    };

    load();

    return () => {
      cancelled = true;
    };
  }, [spritePath]);

  return { imageSrc, imageSize };
}

export default useSpritePreviewImage;