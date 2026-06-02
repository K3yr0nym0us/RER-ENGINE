import type { ReactNode } from 'react';

import { useEffect, useState } from 'react';

import { Accordion } from 'react-bootstrap';

import { Image, LockFill, PlusCircle, Square, Trash, Type, Unlock } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';

import { useTraslate } from '@hooks';

import type {

	EditingUiElement,

	EditingUiElementKind,

} from '../../../../../../context/useContextEngine/types';

import { filterEditingUiElementsByKind } from '../../../../../../context/useContextEngine/types';



interface EditingUiElementGroupsProps {

	elements: EditingUiElement[];

	engineReady: boolean;

	onAddText: () => void;

	onAddButton: () => void;

	onAddImage: () => void;

	onRemoveText: (id: number, label: string) => void;

	onRemoveButton: (id: number, label: string) => void;

	onRemoveImage: (id: number, label: string) => void;

	onSetElementProps: (

		kind: EditingUiElementKind,

		id: number,

		props: { locked?: boolean; z_index?: number },

	) => void;

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



function GroupAddButton({ label, onClick }: { label: string; onClick: () => void }) {

	return (

		<button

			className="btn btn-outline-info btn-sm w-100 mb-2 d-flex align-items-center justify-content-center gap-2"

			type="button"

			onClick={onClick}

		>

			<PlusCircle className="flex-shrink-0" aria-hidden />

			<span>{label}</span>

		</button>

	);

}



function GroupBody({

	addLabel,

	onAdd,

	emptyMessage,

	isEmpty,

	children,

}: {

	addLabel: string;

	onAdd: () => void;

	emptyMessage: string;

	isEmpty: boolean;

	children: ReactNode;

}) {

	return (

		<div className="py-1 px-1">

			<GroupAddButton label={addLabel} onClick={onAdd} />

			{isEmpty ? (

				<p className="small text-secondary text-center mb-0 py-2">{emptyMessage}</p>

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

	onAddButton,

	onAddImage,

	onRemoveText,

	onRemoveButton,

	onRemoveImage,

	onSetElementProps,

}: EditingUiElementGroupsProps) {

	const { t } = useTraslate();



	const lockTooltip = t('Lock element (cannot move in viewport)');

	const unlockTooltip = t('Unlock element');

	const zIndexLabel = t('Z-index');



	const textItems = filterEditingUiElementsByKind(elements, 'text');

	const buttonItems = filterEditingUiElementsByKind(elements, 'button');

	const imageItems = filterEditingUiElementsByKind(elements, 'image');



	const groups: Array<{

		kind: EditingUiElementKind;

		eventKey: string;

		title: string;

		icon: ReactNode;

		addLabel: string;

		onAdd: () => void;

		count: number;

		emptyMessage: string;

		body: ReactNode;

	}> = [

		{

			kind: 'text',

			eventKey: 'ui-el-text',

			title: t('Text'),

			icon: <Type className="me-2 flex-shrink-0" />,

			addLabel: t('Add text'),

			onAdd: onAddText,

			count: textItems.length,

			emptyMessage: t('No text elements on this screen.'),

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

			kind: 'button',

			eventKey: 'ui-el-button',

			title: t('Button'),

			icon: <Square className="me-2 flex-shrink-0" />,

			addLabel: t('Add button'),

			onAdd: onAddButton,

			count: buttonItems.length,

			emptyMessage: t('No button elements on this screen.'),

			body:

				buttonItems.length > 0 ? (

					<div className="d-flex flex-column gap-2">

						{buttonItems.map((item) => {

							const label =

								item.config.text.trim().length > 0

									? item.config.text

									: t('Button');

							return (

								<ElementListRow

									key={`button-${item.id}`}

									label={label}

									locked={item.locked}

									zIndex={item.zIndex}

									engineReady={engineReady}

									lockTooltip={lockTooltip}

									unlockTooltip={unlockTooltip}

									zIndexLabel={zIndexLabel}

									deleteTooltip={t('Delete element')}

									onToggleLock={() =>

										onSetElementProps('button', item.id, {

											locked: !item.locked,

										})

									}

									onZIndexCommit={(z_index) =>

										onSetElementProps('button', item.id, { z_index })

									}

									onDelete={() => onRemoveButton(item.id, label)}

								/>

							);

						})}

					</div>

				) : null,

		},

		{

			kind: 'image',

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



	const defaultActiveKey = groups.filter((g) => g.count > 0).map((g) => g.eventKey);



	return (

		<Accordion

			className="sidebar-accordion mb-2"

			defaultActiveKey={defaultActiveKey.length > 0 ? defaultActiveKey : ['ui-el-text']}

			alwaysOpen

		>

			{groups.map((group) => (

				<Accordion.Item key={group.kind} eventKey={group.eventKey}>

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

						<GroupBody

							addLabel={group.addLabel}

							onAdd={group.onAdd}

							emptyMessage={group.emptyMessage}

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


