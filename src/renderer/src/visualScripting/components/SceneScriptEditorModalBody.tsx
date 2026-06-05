import React, { useMemo, useRef, Suspense } from 'react';

import { useLanguage } from '../../context/LanguageContext';
import { RHAI_MONACO_EDITOR_OPTIONS, RHAI_MONACO_LANGUAGE, registerRhaiMonacoLanguage } from '../../editor/rhaiMonaco';
import { getDefaultSceneScript } from '../../editor/rhaiScriptTemplates';
import { modalTallContentHeightPx } from '../../modal-electron/modalElectronLayout';
import { useTraslate } from '@hooks';

const MonacoEditor = React.lazy(() => import('@monaco-editor/react'));

interface ScriptEditorInstance {
  focus: () => void;
}

interface SceneScriptEditorModalBodyProps {
  initialSource?: string;
  onSave: (source: string) => void;
  onCancel: () => void;
}

export function SceneScriptEditorModalBody({
  initialSource,
  onSave,
  onCancel,
}: SceneScriptEditorModalBodyProps) {
  const { locale } = useLanguage();
  const { t } = useTraslate();
  const defaultScript = useMemo(() => getDefaultSceneScript(locale), [locale]);
  const sourceRef = useRef<string>(initialSource?.trim() ? initialSource : defaultScript);
  const editorRef = useRef<ScriptEditorInstance | null>(null);

  const handleMount = (editor: ScriptEditorInstance) => {
    editorRef.current = editor;
    editor.focus();
  };

  return (
    <div
      className="d-flex flex-column gap-2"
      style={{ height: modalTallContentHeightPx(), minHeight: modalTallContentHeightPx() }}
    >
      <div className="flex-fill rounded overflow-hidden border border-secondary" style={{ minHeight: 0 }}>
        <Suspense fallback={<div>{t('Loading editor...')}</div>}>
          <MonacoEditor
            height="100%"
            language={RHAI_MONACO_LANGUAGE}
            defaultValue={initialSource?.trim() ? initialSource : defaultScript}
            theme="vs-dark"
            beforeMount={registerRhaiMonacoLanguage}
            onChange={(val) => { sourceRef.current = val ?? ''; }}
            onMount={handleMount}
            options={RHAI_MONACO_EDITOR_OPTIONS}
          />
        </Suspense>
      </div>

      <div className="d-flex gap-2 justify-content-end">
        <button type="button" className="btn btn-sm btn-outline-secondary" onClick={onCancel}>
          {t('Cancel')}
        </button>
        <button
          type="button"
          className="btn btn-sm btn-success"
          onClick={() => onSave(sourceRef.current)}
        >
          {t('Save script')}
        </button>
      </div>
    </div>
  );
}

export default SceneScriptEditorModalBody;
