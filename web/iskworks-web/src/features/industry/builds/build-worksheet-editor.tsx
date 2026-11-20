import type { Build } from "../../../api/industry";

import { BuildsPage, ProductSelectionDialog } from "./builds-page";
import { BuildEditorWorkspace } from "./components/build-editor-workspace";
import { useBuildWorksheetEditor } from "./use-build-worksheet-editor";

export function BuildWorksheetEditor({
  initialBuild = null,
  focusedProducer,
  onReload,
}: {
  initialBuild?: Build | null;
  focusedProducer?: Build;
  /** Reload the saved Build from the server (after a server-side rewrite). */
  onReload?: () => void;
}) {
  const editor = useBuildWorksheetEditor(initialBuild);

  if (editor.sdeReady && !editor.selected && !editor.hasRouteBlueprint) {
    return (
      <>
        <BuildsPage />
        <ProductSelectionDialog
          open
          query={editor.query}
          results={editor.productResults}
          searching={editor.productSearching}
          onCancel={() => editor.navigate("/builds")}
          onQuery={editor.setQuery}
          onSelect={editor.selectFromDialog}
        />
      </>
    );
  }

  return <BuildEditorWorkspace editor={editor} focusedProducer={focusedProducer} onReload={onReload} />;
}

export function CreateBuildPage() {
  return <BuildWorksheetEditor />;
}
