import { useEffect, useState } from 'react';

export function useUiButtonTextureUrl(texturePath: string | null): string | null {
	const [dataUrl, setDataUrl] = useState<string | null>(null);

	useEffect(() => {
		let cancelled = false;

		const load = async () => {
			if (!texturePath) {
				setDataUrl(null);
				return;
			}
			const url = await window.electronAPI.getImageDataUrl(texturePath);
			if (!cancelled) setDataUrl(url);
		};

		void load();
		return () => {
			cancelled = true;
		};
	}, [texturePath]);

	return dataUrl;
}
