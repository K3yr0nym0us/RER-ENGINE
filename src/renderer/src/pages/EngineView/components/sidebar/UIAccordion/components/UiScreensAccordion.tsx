import type { ReactNode } from 'react';
import { Accordion } from 'react-bootstrap';
import { Pencil, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import type { UiScreenScope } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import EditingUiElementGroups from './EditingUiElementGroups';
import ModalAddUiButton from './ModalAddUiButton';
import ModalSelectFont from './ModalSelectFont';
import ModalSelectHudImage from './ModalSelectHudImage';
import ModalSetNameUi from './ModalSetNameUi';

interface UiScreensAccordionProps {
	scope: UiScreenScope;
	eventKey: string;
	headerIcon: ReactNode;
	headerTitle: string;
	defaultNamePrefix: string;
}

const UiScreensAccordion = ({
	scope,
	eventKey,
	headerIcon,
	headerTitle,
	defaultNamePrefix,
}: UiScreensAccordionProps) => {
	const { t } = useTraslate();
	const { openModal, closeModal } = useModal();
	const {
		playerUiScreens,
		menuUiScreens,
		playerUiEditingId,
		menuUiEditingId,
		editingUiElements,
		addUiScreen,
		removeUiScreen,
		beginUiScreenEdit,
		endUiScreenEdit,
		addPlayerUiTextBox,
		removePlayerUiTextBox,
		addEditingUiButton,
		addPlayerUiImage,
		removePlayerUiImage,
		removeEditingUiPlaceholder,
		setPlayerUiHudElementProps,
		engineReady,
	} = useContextEngine();

	const screens = scope === 'player' ? playerUiScreens : menuUiScreens;
	const editingId = scope === 'player' ? playerUiEditingId : menuUiEditingId;
	const isEditing = editingId !== null;
	const editingScreen = screens.find((screen) => screen.id === editingId);

	const openNewUiModal = () => {
		const defaultName = `${defaultNamePrefix} ${screens.length + 1}`;
		openModal({
			title: t('New UI'),
			body: (
				<ModalSetNameUi
					defaultName={defaultName}
					onConfirm={(name) => addUiScreen(scope, name)}
				/>
			),
		});
	};

	const openAddTextModal = () => {
		openModal({
			title: t('Add text'),
			body: (
				<ModalSelectFont onSelect={addPlayerUiTextBox} />
			),
		});
	};

	const openAddButtonModal = () => {
		openModal({
			title: t('Add button'),
			size: 'xl',
			body: (
				<ModalAddUiButton onConfirm={addEditingUiButton} />
			),
		});
	};

	const openAddImageModal = () => {
		openModal({
			title: t('Add image'),
			body: <ModalSelectHudImage onSelect={addPlayerUiImage} />,
		});
	};

	const confirmRemoveScreen = (id: string, name: string) => {
		openModal({
			title: t('Confirm deletion'),
			body: (
				<div>
					<p className="mb-3">
						{t('Are you sure you want to delete the UI screen')}{' '}
						<strong>{name}</strong>?
					</p>
					<div className="d-flex justify-content-end gap-2">
						<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
							{t('Cancel')}
						</button>
						<button
							className="btn btn-danger btn-sm"
							type="button"
							onClick={() => {
								if (editingId === id) {
									endUiScreenEdit();
								}
								removeUiScreen(scope, id);
								closeModal();
							}}
						>
							{t('Delete')}
						</button>
					</div>
				</div>
			),
		});
	};

	const confirmRemoveTextBox = (id: number, label: string) => {
		openModal({
			title: t('Confirm deletion'),
			body: (
				<div>
					<p className="mb-3">
						{t('Are you sure you want to delete this text box')}?{' '}
						<strong>{label}</strong>
					</p>
					<div className="d-flex justify-content-end gap-2">
						<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
							{t('Cancel')}
						</button>
						<button
							className="btn btn-danger btn-sm"
							type="button"
							onClick={() => {
								removePlayerUiTextBox(id);
								closeModal();
							}}
						>
							{t('Delete')}
						</button>
					</div>
				</div>
			),
		});
	};

	const confirmRemoveButton = (id: number, label: string) => {
		openModal({
			title: t('Confirm deletion'),
			body: (
				<div>
					<p className="mb-3">
						{t('Are you sure you want to delete this element')}?{' '}
						<strong>{label}</strong>
					</p>
					<div className="d-flex justify-content-end gap-2">
						<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
							{t('Cancel')}
						</button>
						<button
							className="btn btn-danger btn-sm"
							type="button"
							onClick={() => {
								removeEditingUiPlaceholder('button', id);
								closeModal();
							}}
						>
							{t('Delete')}
						</button>
					</div>
				</div>
			),
		});
	};

	const confirmRemoveImage = (id: number, label: string) => {
		openModal({
			title: t('Confirm deletion'),
			body: (
				<div>
					<p className="mb-3">
						{t('Are you sure you want to delete this element')}?{' '}
						<strong>{label}</strong>
					</p>
					<div className="d-flex justify-content-end gap-2">
						<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
							{t('Cancel')}
						</button>
						<button
							className="btn btn-danger btn-sm"
							type="button"
							onClick={() => {
								removePlayerUiImage(id);
								closeModal();
							}}
						>
							{t('Delete')}
						</button>
					</div>
				</div>
			),
		});
	};

	if (isEditing) {
		return (
			<Accordion.Item eventKey={eventKey}>
				<Accordion.Header>
					{headerIcon}
					{headerTitle}
				</Accordion.Header>
				<Accordion.Body className="py-2 px-2">
					{editingScreen && (
						<p className="small text-secondary mb-2 text-truncate">
							{t('Editing')}: <strong className="text-light">{editingScreen.name}</strong>
						</p>
					)}
					<p className="small text-secondary mb-2">
						{t('Double-click a text box in the viewport to edit. Backspace removes characters.')}
					</p>

					<EditingUiElementGroups
						elements={editingUiElements}
						engineReady={engineReady}
						onAddText={openAddTextModal}
						onAddButton={openAddButtonModal}
						onAddImage={openAddImageModal}
						onRemoveText={confirmRemoveTextBox}
						onRemoveButton={confirmRemoveButton}
						onRemoveImage={confirmRemoveImage}
						onSetElementProps={setPlayerUiHudElementProps}
					/>

					<div className="d-flex gap-2">
						<button
							className="btn btn-outline-secondary btn-sm flex-fill"
							type="button"
							onClick={endUiScreenEdit}
						>
							{t('Cancel')}
						</button>
						<button
							className="btn btn-primary btn-sm flex-fill"
							type="button"
							onClick={endUiScreenEdit}
						>
							{t('Save')}
						</button>
					</div>
				</Accordion.Body>
			</Accordion.Item>
		);
	}

	return (
		<Accordion.Item eventKey={eventKey}>
			<Accordion.Header>
				{headerIcon}
				{headerTitle}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<button
					className="btn btn-outline-success btn-sm w-100 fw-bold mb-2"
					type="button"
					onClick={openNewUiModal}
				>
					{t('+ New UI')}
				</button>

				{screens.length === 0 && (
					<p className="small text-secondary text-center mb-0 py-2">
						{t('No UI screens.')}
					</p>
				)}

				{screens.length > 0 && (
					<div className="d-flex flex-column gap-2">
						{screens.map((screen) => (
							<div
								key={screen.id}
								className="d-flex align-items-center gap-2 p-2 pt-1 pb-1 border border-secondary rounded bg-dark"
							>
								<AppTooltip content={screen.name} place="top">
									<span className="small fw-semibold text-light flex-fill text-truncate">
										{screen.name}
									</span>
								</AppTooltip>

								<AppTooltip content={t('Edit UI')} place="top">
									<span
										role="button"
										tabIndex={0}
										className="text-warning"
										style={{ cursor: 'pointer' }}
										onClick={() => beginUiScreenEdit(scope, screen.id)}
										onKeyDown={(e) => {
											if (e.key === 'Enter' || e.key === ' ') {
												beginUiScreenEdit(scope, screen.id);
											}
										}}
									>
										<Pencil />
									</span>
								</AppTooltip>

								<AppTooltip content={t('Delete UI')} place="top">
									<span
										role="button"
										tabIndex={0}
										className="text-danger"
										style={{ cursor: 'pointer' }}
										onClick={() => confirmRemoveScreen(screen.id, screen.name)}
										onKeyDown={(e) => {
											if (e.key === 'Enter' || e.key === ' ') {
												confirmRemoveScreen(screen.id, screen.name);
											}
										}}
									>
										<Trash />
									</span>
								</AppTooltip>
							</div>
						))}
					</div>
				)}
			</Accordion.Body>
		</Accordion.Item>
	);
};

export default UiScreensAccordion;
