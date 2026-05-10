export const CANVAS_SIZE = 500;

export type SelectionMode = 'cell' | 'box';

export interface ScriptEntry {
  name:   string;
  source: string;
}

export interface SpriteFrameRect {
  x: number;
  y: number;
  width: number;
  height: number;
  pivot_x?: number;
  pivot_y?: number;
}

export interface SpritePreviewConfirmConfig {
  animationName: string;
  frames: SpriteFrameRect[];
  fps: number;
  loop: boolean;
  audioPath?: string;
  scripts: ScriptEntry[];
}

export interface SpritePreviewState {
  animationName: string;
  validationError: string | null;
  cellOffsetX: number;
  cellOffsetY: number;
  gridSize: number;
  selectionMode: SelectionMode;
  selectedCells: { x: number; y: number }[];
  boxes: { x: number; y: number; width: number; height: number }[];
  currentBox: { x: number; y: number; width: number; height: number };
  fps: number;
  isLooping: boolean;
  audioPath?: string;
  scripts: ScriptEntry[];
  isCancelable: boolean;
}

export type SpritePreviewAction =
  | { type: 'patch'; payload: Partial<SpritePreviewState> }
  | { type: 'toggle_cell'; payload: { x: number; y: number } }
  | { type: 'append_current_box' }
  | { type: 'remove_box'; payload: number }
  | { type: 'pop_box' };

export const initialSpritePreviewState: SpritePreviewState = {
  animationName: '',
  validationError: null,
  cellOffsetX: 0,
  cellOffsetY: 0,
  gridSize: 32,
  selectionMode: 'cell',
  selectedCells: [],
  boxes: [],
  currentBox: { x: 0, y: 0, width: 64, height: 64 },
  fps: 12,
  isLooping: false,
  audioPath: undefined,
  scripts: [],
  isCancelable: false,
};

export function spritePreviewReducer(
  state: SpritePreviewState,
  action: SpritePreviewAction,
): SpritePreviewState {
  switch (action.type) {
    case 'patch':
      return { ...state, ...action.payload };
    case 'toggle_cell': {
      const { x, y } = action.payload;
      const exists = state.selectedCells.some((cell) => cell.x === x && cell.y === y);
      return {
        ...state,
        selectedCells: exists
          ? state.selectedCells.filter((cell) => !(cell.x === x && cell.y === y))
          : [...state.selectedCells, { x, y }],
      };
    }
    case 'append_current_box':
      return { ...state, boxes: [...state.boxes, { ...state.currentBox }] };
    case 'remove_box':
      return { ...state, boxes: state.boxes.filter((_, i) => i !== action.payload) };
    case 'pop_box':
      return { ...state, boxes: state.boxes.slice(0, -1) };
    default:
      return state;
  }
}
