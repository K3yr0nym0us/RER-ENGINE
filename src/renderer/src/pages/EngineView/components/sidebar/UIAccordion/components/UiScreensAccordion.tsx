import type { ReactNode } from 'react';
import { useEffect, useRef, useState } from 'react';
import { Accordion, Form } from 'react-bootstrap';
import { Check2Square, Pencil, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import type { UiScreenScope } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { ModalConfirmBody } from '../../../../../../modal-electron/ModalConfirmBody';
import { usePlayerUiObjectDrawing } from '@hooks';
import { usePlayerUiEditorModal } from '../../../../../../modal-electron/usePlayerUiEditorModal';
import EditingUiElementGroups from './EditingUiElementGroups';
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
	const { openModal } = useModal();
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
		addPlayerUiImage,
		removePlayerUiImage,
		removePlayerUiObject,
		setPlayerUiHudElementProps,
		setPlayerUiObjectStyle,
		send,
		playerUiObjectDrawEndTick,
		setActivePlayerUiScreen,
		syncPlayerUiScreensToEngine,
		renameUiScreen,
		engineReady,
	} = useContextEngine();
	const objectDraw = usePlayerUiObjectDrawing(send, playerUiObjectDrawEndTick);
	const playerUiEditor = usePlayerUiEditorModal(scope);

	const finishUiScreenEdit = () => {
		objectDraw.cancel();
		endUiScreenEdit();
	};

	const [uiNameDraft, setUiNameDraft] = useState('');
	const [isEditingUiName, setIsEditingUiName] = useState(false);

	const screens = scope === 'player' ? playerUiScreens : menuUiScreens;
	const editingId = scope === 'player' ? playerUiEditingId : menuUiEditingId;
	const isEditing = editingId !== null;
	const editingScreen = screens.find((screen) => screen.id === editingId);
	const syncScreensRef = useRef(syncPlayerUiScreensToEngine);
	syncScreensRef.current = syncPlayerUiScreensToEngine;

	useEffect(() => {
		if (editingScreen) {
			setUiNameDraft(editingScreen.name);
			setIsEditingUiName(false);
		}
	}, [editingScreen?.id, editingScreen?.name]);

	useEffect(() => {
		if (scope === 'player') {
			syncScreensRef.current(playerUiScreens);
		}
	}, [scope, playerUiScreens]);

	const openNewUiModal = () => {
		const defaultName = `${defaultNamePrefix} ${screens.length + 1}`;
		openModal({
			title: t('New UI'),
			body: (
				<ModalSetNameUi
					defaultName={defaultName}
					onConfirm={(name) => {
						const id = addUiScreen(scope, name);
						if (scope === 'player' && id) {
							playerUiEditor.openEditor(id);
						}
					}}
				/>
			),
		});
	};

	const openAddTextModal = () => {
		openModal({
			title: t('Add text'),
			size: 'sm',
			body: (
				<ModalSelectFont onSelect={addPlayerUiTextBox} />
			),
		});
	};

	const openAddImageModal = () => {
		openModal({
			title: t('Add image'),
			body: <ModalSelectHudImage onSelect={addPlayerUiImage} />,
		});
	};

	const openAssignObjectTextureModal = (objectId: number) => {
		openModal({
			title: t('Assign texture'),
			body: (
				<ModalSelectHudImage
					onSelect={(imagePath) => {
						setPlayerUiObjectStyle(objectId, { texture_path: imagePath });
					}}
				/>
			),
		});
	};

	const confirmRemoveScreen = (id: string, name: string) => {
		openModal({
			title: t('Confirm deletion'),
			size: 'sm',
			body: (
				<ModalConfirmBody
					buttonSize="sm"
					message={
						<>
							{t('Are you sure you want to delete the UI screen')}{' '}
							<strong>{name}</strong>?
						</>
					}
					onConfirm={() => {
						if (editingId === id) {
							if (scope === 'player') {
								void window.electronAPI.closeModalElectron();
								endUiScreenEdit();
							} else {
								finishUiScreenEdit();
							}
						}
						removeUiScreen(scope, id);
					}}
				/>
			),
		});
	};

	const confirmRemoveTextBox = (id: number, label: string) => {
		openModal({
			title: t('Confirm deletion'),
			size: 'sm',
			body: (
				<ModalConfirmBody
					buttonSize="sm"
					message={
						<>
							{t('Are you sure you want to delete this text box')}?{' '}
							<strong>{label}</strong>
						</>
					}
					onConfirm={() => removePlayerUiTextBox(id)}
				/>
			),
		});
	};

	const confirmRemoveObject = (id: number, label: string) => {
		openModal({
			title: t('Confirm deletion'),
			size: 'sm',
			body: (
				<ModalConfirmBody
					buttonSize="sm"
					message={
						<>
							{t('Are you sure you want to delete this element')}?{' '}
							<strong>{label}</strong>
						</>
					}
					onConfirm={() => removePlayerUiObject(id)}
				/>
			),
		});
	};

	const confirmRemoveImage = (id: number, label: string) => {
		openModal({
			title: t('Confirm deletion'),
			size: 'sm',
			body: (
				<ModalConfirmBody
					buttonSize="sm"
					message={
						<>
							{t('Are you sure you want to delete this element')}?{' '}
							<strong>{label}</strong>
						</>
					}
					onConfirm={() => removePlayerUiImage(id)}
				/>
			),
		});
	};

	if (isEditing && scope !== 'player') {
		return (
			<Accordion.Item eventKey={eventKey}>
				<Accordion.Header>
					{headerIcon}
					{headerTitle}
				</Accordion.Header>
				<Accordion.Body className="py-2 px-2">
					{editingScreen && (
						<div className="mb-2">
							<p className="prop-label small text-secondary mb-1">{t('UI name')}</p>
							<div className="input-group input-group-sm">
								<input
									type="text"
									value={uiNameDraft}
									onChange={(e) => setUiNameDraft(e.target.value)}
									className="form-control bg-dark text-info border-secondary prop-input"
									aria-label={t('UI name')}
									disabled={!isEditingUiName}
								/>
								{!isEditingUiName ? (
									<AppTooltip content={t('Edit name')} place="top">
										<button
											type="button"
											className="btn btn-outline-secondary"
											onClick={() => setIsEditingUiName(true)}
										>
											<Pencil />
										</button>
									</AppTooltip>
								) : (
									<AppTooltip content={t('Save changes')} place="top">
										<button
											type="button"
											className="btn btn-outline-info"
											disabled={!uiNameDraft.trim()}
											onClick={() => {
												const trimmed = uiNameDraft.trim();
												if (!trimmed || !editingId) return;
												renameUiScreen(scope, editingId, trimmed);
												setIsEditingUiName(false);
											}}
										>
											<Check2Square />
										</button>
									</AppTooltip>
								)}
							</div>
						</div>
					)}

					<EditingUiElementGroups
						elements={editingUiElements}
						engineReady={engineReady}
						onAddText={openAddTextModal}
						onAddImage={openAddImageModal}
						onAddObject={objectDraw.start}
						onCancelObjectDraw={objectDraw.cancel}
						onRemoveText={confirmRemoveTextBox}
						onRemoveImage={confirmRemoveImage}
						onRemoveObject={confirmRemoveObject}
						objectDrawActive={objectDraw.isActive}
						onSetElementProps={setPlayerUiHudElementProps}
						onSetObjectStyle={(id, fillColor, options) =>
							setPlayerUiObjectStyle(id, { fill_color: fillColor, ...options })
						}
						onAssignObjectTexture={openAssignObjectTextureModal}
						textEditHint={t(
							'Double-click a text box in the viewport to edit. Backspace removes characters. Hold Ctrl while dragging to snap to the grid.',
						)}
					/>

					<div className="d-flex gap-2">
						<button
							className="btn btn-outline-secondary btn-sm flex-fill"
							type="button"
							onClick={finishUiScreenEdit}
						>
							{t('Cancel')}
						</button>
						<button
							className="btn btn-primary btn-sm flex-fill"
							type="button"
							onClick={finishUiScreenEdit}
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

								{scope === 'player' ? (
									<AppTooltip content={t('Active HUD in play')} place="top">
										<Form.Check
											type="checkbox"
											className="m-0 flex-shrink-0"
											checked={Boolean(screen.active)}
											disabled={!engineReady}
											aria-label={t('Active HUD in play')}
											onChange={() => {
												setActivePlayerUiScreen(
													screen.active ? null : screen.id,
												);
											}}
										/>
									</AppTooltip>
								) : null}

								<AppTooltip content={t('Edit UI')} place="top">
									<span
										role="button"
										tabIndex={0}
										className="text-warning"
										style={{ cursor: 'pointer' }}
										onClick={() => {
											if (scope === 'player') {
												playerUiEditor.openEditor(screen.id);
											} else {
												beginUiScreenEdit(scope, screen.id);
											}
										}}
										onKeyDown={(e) => {
											if (e.key === 'Enter' || e.key === ' ') {
												if (scope === 'player') {
													playerUiEditor.openEditor(screen.id);
												} else {
													beginUiScreenEdit(scope, screen.id);
												}
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
