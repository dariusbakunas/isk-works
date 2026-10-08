import { UserRound } from "lucide-react";
import { useEffect, useState } from "react";

import { eveCharacterPortraitUrl } from "./eve-image-url";

export function EveCharacterPortrait({
  characterId,
  characterName,
  size = 40,
  className = "",
}: {
  characterId: number;
  characterName: string;
  size?: 16 | 32 | 40 | 48 | 64 | 128 | 256;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const requestSize = size <= 32 ? 32 : size <= 64 ? 64 : size <= 128 ? 128 : 256;
  const frameStyle = { width: size, height: size };

  useEffect(() => setFailed(false), [characterId, requestSize]);

  // `data-private` keeps the portrait (account-linked character identity)
  // out of LogRocket session replay; the alt/aria-label deliberately omit
  // the character name so it is not captured as an attribute either.
  if (failed) {
    return (
      <span
        aria-label="Character portrait unavailable"
        className={`grid shrink-0 place-items-center overflow-hidden rounded border border-border bg-panel-strong text-muted ${className}`}
        data-private=""
        role="img"
        style={frameStyle}
      >
        <UserRound aria-hidden="true" className="h-1/2 w-1/2" />
      </span>
    );
  }

  return (
    <img
      alt="Character portrait"
      className={`shrink-0 rounded border border-border bg-panel-strong object-cover ${className}`}
      data-private=""
      decoding="async"
      height={size}
      loading="lazy"
      onError={() => setFailed(true)}
      src={eveCharacterPortraitUrl(characterId, requestSize)}
      style={frameStyle}
      width={size}
    />
  );
}
