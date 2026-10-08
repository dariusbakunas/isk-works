import { ArrowLeft } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import { getBuild, type Build } from "../../../api/industry";
import { InlineAlert, Panel } from "../../../components/primitives";
import { apiMessage } from "../shared/api-error";
import { BuildWorksheetEditor } from "./build-worksheet-editor";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: Build; focusedProducer?: Build };

export function BuildWorkspacePage({ canonicalize = false }: { canonicalize?: boolean }) {
  const { buildId = "", rootBuildId = "", producerBuildId = "" } = useParams();
  const navigate = useNavigate();
  const [state, setState] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    const requestedBuildId = rootBuildId || buildId;
    Promise.all([
      getBuild(requestedBuildId),
      producerBuildId ? getBuild(producerBuildId) : Promise.resolve(null),
    ])
      .then(([build, focusedProducer]) => {
        if (focusedProducer) {
          const validContext =
            build.id === rootBuildId &&
            build.planRootBuildId === rootBuildId &&
            focusedProducer.id !== rootBuildId &&
            focusedProducer.planRootBuildId === rootBuildId;
          if (!validContext) {
            setState({
              status: "error",
              message: "Focused producer unavailable: this producer does not belong to the requested top-level Build plan.",
            });
            return;
          }
          setState({ status: "ready", data: build, focusedProducer });
          return;
        }
        if (build.planRootBuildId && build.planRootBuildId !== build.id) {
          navigate(`/builds/${build.planRootBuildId}/producers/${build.id}`, { replace: true });
          return;
        }
        setState({ status: "ready", data: build });
        // The legacy `/edit` link means "configure this Build": land on the
        // Plan with Build settings (root configuration) open.
        if (canonicalize) navigate(`/builds/${build.id}?view=plan&settings=open`, { replace: true });
      })
      .catch((error) => setState({ status: "error", message: apiMessage(error) }));
  }, [buildId, rootBuildId, producerBuildId, canonicalize, navigate]);

  if (state.status === "loading") return <Panel>Loading Build...</Panel>;
  if (state.status === "error") {
    return <InlineAlert title={producerBuildId ? "Focused producer unavailable" : "Build unavailable"}>{state.message}</InlineAlert>;
  }
  return (
    <>
      {state.focusedProducer ? (
        <Link
          className="mb-3 inline-flex items-center gap-1.5 text-sm text-muted hover:text-foreground"
          to={`/builds/${state.data.id}?focusProducer=${state.focusedProducer.id}`}
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          {state.data.name}
        </Link>
      ) : null}
      {
        // `key` forces a remount on every distinct build id: BuildWorksheetEditor
        // seeds its internal state from `initialBuild` via useState initializers
        // (which only run once), so navigating directly between two builds'
        // worksheets -- e.g. via the producer breadcrumb above -- would
        // otherwise leave the worksheet body showing stale data from the
        // previous build even though this route stays mounted throughout.
      }
      <BuildWorksheetEditor
        focusedProducer={state.focusedProducer}
        initialBuild={state.data}
        key={`${state.data.id}:${state.focusedProducer?.id ?? "root"}`}
      />
    </>
  );
}
