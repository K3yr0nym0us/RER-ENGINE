import type { EditingUiElement, EditingUiElementKind } from '@engine'

export interface PlayerUiEditorState {
	screenId: string
	screenName: string
	elements: EditingUiElement[]
	engineReady: boolean
	objectDrawActive: boolean
}

export type PlayerUiEditorAction =
	| { action: 'rename'; name: string }
	| { action: 'setElementProps'; kind: EditingUiElementKind; id: number; props: { locked?: boolean; z_index?: number } }
	| {
			action: 'setObjectStyle';
			id: number;
			fill_color: [number, number, number, number];
			live?: boolean;
			skip_undo?: boolean;
	  }
	| { action: 'assignObjectTexture'; id: number }
	| { action: 'addText' }
	| { action: 'addImage' }
	| { action: 'objectDrawStart' }
	| { action: 'objectDrawCancel' }
	| { action: 'removeText'; id: number; label: string }
	| { action: 'removeImage'; id: number; label: string }
	| { action: 'removeObject'; id: number; label: string }
	| { action: 'save' }
	| { action: 'cancel' }
