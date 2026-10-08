import { Package } from "lucide-react";
import { useEffect, useState } from "react";

import { eveTypeImageUrl, type EveTypeImageVariation } from "./eve-image-url";

export type { EveTypeImageVariation } from "./eve-image-url";

export function EveTypeImage({
  typeId,
  typeName,
  variation = "icon",
  size = 40,
  className = "",
}: {
  typeId: number;
  typeName: string;
  variation?: EveTypeImageVariation;
  size?: 24 | 32 | 40 | 48 | 64 | 128 | 256;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const [activeVariation, setActiveVariation] = useState(variation);
  const requestSize = size <= 32 ? 32 : size <= 64 ? 64 : size <= 128 ? 128 : 256;

  useEffect(() => {
    setFailed(false);
    setActiveVariation(variation);
  }, [typeId, variation, requestSize]);

  const frameStyle = { width: size, height: size };
  if (failed) {
    return (
      <span
        aria-label={`${typeName} image unavailable`}
        className={`grid shrink-0 place-items-center overflow-hidden rounded border border-border bg-panel-strong text-muted ${className}`}
        role="img"
        style={frameStyle}
      >
        <Package aria-hidden="true" className="h-1/2 w-1/2" />
      </span>
    );
  }

  return (
    <img
      alt={typeName}
      className={`shrink-0 rounded border border-border bg-panel-strong object-contain ${className}`}
      decoding="async"
      height={size}
      loading="lazy"
      onError={() => {
        if (activeVariation !== "icon") setActiveVariation("icon");
        else setFailed(true);
      }}
      src={eveTypeImageUrl(typeId, activeVariation, requestSize)}
      style={frameStyle}
      width={size}
    />
  );
}
