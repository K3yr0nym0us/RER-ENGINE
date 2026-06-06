import type { ReactNode } from 'react';

import { useEffect, useState } from 'react';

import { Accordion } from 'react-bootstrap';

import { Box, Image, LockFill, PlusCircle, Trash, Type, Unlock } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';

import { useTraslate } from '@hooks';

import type { EditingUiElement, EditingUiElementKind } from '@engine';
import { filterEditingUiElementsByKind } from '@engine';
import ObjectElementAccordionRow from './ObjectElementAccordionRow';
import type { FillColorRgba, PlayerUiObjectStyleCommitOptions } from './playerUiObjectStyle';



interface EditingUiElementGroupsProps {

	elements: EditingUiElement[];

	engineReady: boolean;

	onAddText: () => void;

	onAddImage: () => void;

	onAddObject: () => void;

	onCancelObjectDraw: () => void;

	onRemoveText: (id: number, label: string) => void;

	onRemoveImage: (id: number, label: string) => void;

	onRemoveObject: (id: number, label: string) => void;

	objectDrawActive: boolean;

	/** Ayuda de edición de texto (viewport). */
	textEditHint?: string;

	onSetElementProps: (

		kind: EditingUiElementKind,

		id: number,

		props: { locked?: boolean; z_index?: number },

	) => void;

	onSetObjectStyle: (
		id: number,
		fillColor: FillColorRgba,
		options?: PlayerUiObjectStyleCommitOptions,
	) => void;

	onAssignObjectTexture: (id: number) => void;

}



function ElementListRow({

	label,

	locked,

	zIndex,

	engineReady,

	lockTooltip,

	unlockTooltip,

	zIndexLabel,

	deleteTooltip,

	onToggleLock,

	onZIndexCommit,

	onDelete,

}: {

	label: string;

	locked: boolean;

	zIndex: number;

	engineReady: boolean;

	lockTooltip: string;

	unlockTooltip: string;

	zIndexLabel: string;

	deleteTooltip: string;

	onToggleLock: () => void;

	onZIndexCommit: (value: number) => void;

	onDelete: () => void;

}) {

	const [zDraft, setZDraft] = useState(String(zIndex));

	useEffect(() => {
		setZDraft(String(zIndex));
	}, [zIndex]);

	const commitZ = () => {

		const parsed = Number.parseInt(zDraft, 10);

		const next = Number.isFinite(parsed) ? parsed : 0;

		setZDraft(String(next));

		if (next !== zIndex) {

			onZIndexCommit(next);

		}

	};



	return (

		<div className="d-flex align-items-center gap-1 p-2 pt-1 pb-1 border border-secondary rounded bg-dark">

			<AppTooltip content={label} place="top">

				<span className="small text-light flex-fill text-truncate">{label}</span>

			</AppTooltip>

			<AppTooltip content={locked ? unlockTooltip : lockTooltip} place="top">

				<span

					role="button"

					tabIndex={0}

					className={locked ? 'text-warning' : 'text-secondary'}

					style={{ cursor: engineReady ? 'pointer' : 'not-allowed' }}

					onClick={() => {

						if (engineReady) onToggleLock();

					}}

					onKeyDown={(e) => {

						if (engineReady && (e.key === 'Enter' || e.key === ' ')) onToggleLock();

					}}

				>

					{locked ? <LockFill /> : <Unlock />}

				</span>

			</AppTooltip>

			<AppTooltip content={zIndexLabel} place="top">

				<input

					type="number"

					className="form-control form-control-sm bg-dark text-light border-secondary py-0 px-1"

					style={{ width: '3.1rem' }}

					title={zIndexLabel}

					aria-label={zIndexLabel}

					disabled={!engineReady}

					value={zDraft}

					onChange={(e) => setZDraft(e.target.value)}

					onBlur={commitZ}

					onKeyDown={(e) => {

						if (e.key === 'Enter') {

							commitZ();

							(e.target as HTMLInputElement).blur();

						}

					}}

				/>

			</AppTooltip>

			<AppTooltip content={deleteTooltip} place="top">

				<span

					role="button"

					tabIndex={0}

					className="text-danger"

					style={{ cursor: 'pointer' }}

					onClick={onDelete}

					onKeyDown={(e) => {

						if (e.key === 'Enter' || e.key === ' ') onDelete();

					}}

				>

					<Trash />

				</span>

			</AppTooltip>

		</div>

	);

}



function GroupAddButton({
	label,
	onClick,
	disabled,
}: {
	label: string;
	onClick: () => void;
	disabled?: boolean;
}) {

	return (

		<button

			className="btn btn-outline-info btn-sm w-100 mb-2 d-flex align-items-center justify-content-center gap-2"

			type="button"

			onClick={onClick}

			disabled={disabled}

		>

			<PlusCircle className="flex-shrink-0" aria-hidden />

			<span>{label}</span>

		</button>

	);

}



function GroupBody({

	addLabel,

	onAdd,

	addDisabled,

	emptyMessage,

	isEmpty,

	emptyMessageClassName,

	children,

}: {

	addLabel: string;

	onAdd: () => void;

	addDisabled?: boolean;

	emptyMessage: string;

	isEmpty: boolean;

	emptyMessageClassName?: string;

	children: ReactNode;

}) {

	return (

		<div className="py-1 px-1">

			<GroupAddButton label={addLabel} onClick={onAdd} disabled={addDisabled} />

			{isEmpty ? (

				<p className={`small text-center mb-0 py-2 ${emptyMessageClassName ?? 'text-secondary'}`}>{emptyMessage}</p>

			) : (

				children

			)}

		</div>

	);

}



export default function EditingUiElementGroups({

	elements,

	engineReady,

	onAddText,

	onAddImage,

	onAddObject,

	onCancelObjectDraw,

	onRemoveText,

	onRemoveImage,

	onRemoveObject,

	onSetElementProps,

	onSetObjectStyle,

	onAssignObjectTexture,

	textEditHint,

	objectDrawActive,

}: EditingUiElementGroupsProps) {

	const { t } = useTraslate();



	const lockTooltip = t('Lock element (cannot move in viewport)');

	const unlockTooltip = t('Unlock element');

	const zIndexLabel = t('Z-index');



	const textItems = filterEditingUiElementsByKind(elements, 'text');

	const imageItems = filterEditingUiElementsByKind(elements, 'image');
	const objectItems = filterEditingUiElementsByKind(elements, 'object');



	const groups: Array<{

		eventKey: string;

		title: string;

		icon: ReactNode;

		addLabel: string;

		onAdd: () => void;

		count: number;

		emptyMessage: string;

		emptyMessageClassName?: string;

		body: ReactNode;

		preBody?: ReactNode;

		addDisabled?: boolean;

	}> = [

		{

			eventKey: 'ui-el-text',

			title: t('Text'),

			icon: <Type className="me-2 flex-shrink-0" />,

			addLabel: t('Add text'),

			onAdd: onAddText,

			count: textItems.length,

			emptyMessage: t('No text elements on this screen.'),

			preBody: textEditHint ? (

				<p className="small text-secondary mb-2 px-1">{textEditHint}</p>

			) : null,

			body:

				textItems.length > 0 ? (

					<div className="d-flex flex-column gap-2">

						{textItems.map((box) => {

							const label =

								box.text.trim().length > 0 ? box.text : t('Empty text');

							return (

								<ElementListRow

									key={`text-${box.id}`}

									label={label}

									locked={box.locked}

									zIndex={box.zIndex}

									engineReady={engineReady}

									lockTooltip={lockTooltip}

									unlockTooltip={unlockTooltip}

									zIndexLabel={zIndexLabel}

									deleteTooltip={t('Delete text box')}

									onToggleLock={() =>

										onSetElementProps('text', box.id, { locked: !box.locked })

									}

									onZIndexCommit={(z_index) =>

										onSetElementProps('text', box.id, { z_index })

									}

									onDelete={() => onRemoveText(box.id, label)}

								/>

							);

						})}

					</div>

				) : null,

		},

		{

			eventKey: 'ui-el-object',

			title: t('Object'),

			icon: <Box className="me-2 flex-shrink-0" />,

			addLabel: objectDrawActive ? t('Cancel drawing') : t('Add object'),

			onAdd: objectDrawActive ? onCancelObjectDraw : onAddObject,

			count: objectItems.length,

			emptyMessage: objectDrawActive
				? t('Click to add points; click the first point again to finish (min. 3). Hold Ctrl to snap to grid. Esc cancels.')
				: t('No objects on this screen.'),

			emptyMessageClassName: objectDrawActive ? 'text-danger' : undefined,

			body:
				objectItems.length > 0 ? (
					<div className="d-flex flex-column">
						{objectItems.map((item) => {
							const label = `${t('Object')} #${item.id}`;
							return (
								<ObjectElementAccordionRow
									key={`object-${item.id}`}
									item={item}
									engineReady={engineReady}
									lockTooltip={lockTooltip}
									unlockTooltip={unlockTooltip}
									zIndexLabel={zIndexLabel}
									deleteTooltip={t('Delete element')}
									onToggleLock={() =>
										onSetElementProps('object', item.id, {
											locked: !item.locked,
										})
									}
									onZIndexCommit={(z_index) =>
										onSetElementProps('object', item.id, { z_index })
									}
									onDelete={() => onRemoveObject(item.id, label)}
									onAssignTexture={() => onAssignObjectTexture(item.id)}
									onStyleChange={(fillColor, options) =>
										onSetObjectStyle(item.id, fillColor, options)
									}
								/>
							);
						})}
					</div>
				) : null,

		},

		{

			eventKey: 'ui-el-image',

			title: t('Image'),

			icon: <Image className="me-2 flex-shrink-0" />,

			addLabel: t('Add image'),

			onAdd: onAddImage,

			count: imageItems.length,

			emptyMessage: t('No image elements on this screen.'),

			body:

				imageItems.length > 0 ? (

					<div className="d-flex flex-column gap-2">

						{imageItems.map((item) => {

							const label =

								item.imageName.trim().length > 0 ? item.imageName : t('Image');

							return (

								<ElementListRow

									key={`image-${item.id}`}

									label={label}

									locked={item.locked}

									zIndex={item.zIndex}

									engineReady={engineReady}

									lockTooltip={lockTooltip}

									unlockTooltip={unlockTooltip}

									zIndexLabel={zIndexLabel}

									deleteTooltip={t('Delete element')}

									onToggleLock={() =>

										onSetElementProps('image', item.id, {

											locked: !item.locked,

										})

									}

									onZIndexCommit={(z_index) =>

										onSetElementProps('image', item.id, { z_index })

									}

									onDelete={() => onRemoveImage(item.id, label)}

								/>

							);

						})}

					</div>

				) : null,

		},

	];



	const [activeGroupKey, setActiveGroupKey] = useState<string | null>(null);

	useEffect(() => {
		if (objectDrawActive) {
			setActiveGroupKey('ui-el-object');
		}
	}, [objectDrawActive]);

	return (
		<Accordion
			className="sidebar-accordion mb-2"
			activeKey={activeGroupKey ?? undefined}
			onSelect={(key) => setActiveGroupKey(typeof key === 'string' ? key : null)}
		>

			{groups.map((group) => (

				<Accordion.Item key={group.eventKey} eventKey={group.eventKey}>

					<Accordion.Header className="py-1">

						<span className="d-flex align-items-center small">

							{group.icon}

							<span>

								{group.title}

								<span className="text-secondary ms-1">({group.count})</span>

							</span>

						</span>

					</Accordion.Header>

					<Accordion.Body className="py-0 px-1">

						{group.preBody}

						<GroupBody

							addLabel={group.addLabel}

							onAdd={group.onAdd}

							addDisabled={group.addDisabled}

							emptyMessage={group.emptyMessage}

							emptyMessageClassName={group.emptyMessageClassName}

							isEmpty={group.count === 0}

						>

							{group.body}

						</GroupBody>

					</Accordion.Body>

				</Accordion.Item>

			))}

		</Accordion>

	);

}


