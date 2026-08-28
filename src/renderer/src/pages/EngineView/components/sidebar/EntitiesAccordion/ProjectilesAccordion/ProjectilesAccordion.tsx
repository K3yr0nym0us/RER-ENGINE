import { PlusLg } from 'react-bootstrap-icons';

import { CreateProjectileFromSpriteModalBody } from '../components/CreateProjectileFromSpriteModalBody';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';

export function ProjectilesAccordion() {
	const { t } = useTraslate();
	const { engineReady, sendAsync } = useContextEngine();
	const { openModal } = useModal();

	const handleCreateProjectile = () => {
		openModal({
			title: t('Create projectile'),
			body: (
				<CreateProjectileFromSpriteModalBody
					onConfirm={({ spritePath }) => {
						void sendAsync<{ id: number }>(
							{ cmd: 'load_projectile', path: spritePath },
							'projectile_loaded',
						);
					}}
				/>
			),
		});
	};

	return (
		<button
			className="btn btn-outline-info btn-sm w-100 fw-bold mb-2"
			disabled={!engineReady}
			onClick={handleCreateProjectile}
		>
			<PlusLg className="me-2" />
			{t('Create projectile')}
		</button>
	);
}

export default ProjectilesAccordion;
