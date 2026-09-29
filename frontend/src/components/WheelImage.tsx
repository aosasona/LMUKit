import { invoke } from "@tauri-apps/api/core";
import { Gamepad2 } from "lucide-react";
import { useEffect, useState } from "react";

export function WheelImage({
  wheelId,
  hasImage,
  alt,
  className,
  revision = 0,
}: {
  wheelId: string | null;
  hasImage: boolean;
  alt: string;
  className?: string;
  revision?: number;
}) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let objectUrl: string | null = null;
    let cancelled = false;
    setUrl(null);
    if (!wheelId || !hasImage) return;
    void invoke<number[] | null>("wheel_image", { wheelId })
      .then((bytes) => {
        if (!bytes || cancelled) return;
        const data = new Uint8Array(bytes);
        const type =
          data[0] === 0x89
            ? "image/png"
            : data[0] === 0xff
              ? "image/jpeg"
              : "image/webp";
        objectUrl = URL.createObjectURL(new Blob([data], { type }));
        setUrl(objectUrl);
      })
      .catch(() => setUrl(null));
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [wheelId, hasImage, revision]);

  return url ? (
    <img className={className} src={url} alt={alt} />
  ) : (
    <span className={className} aria-label={alt}>
      <Gamepad2 />
    </span>
  );
}
