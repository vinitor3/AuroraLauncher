/** Legacy 64x32 PNGs often fill the unused hat area with opaque black. Minecraft
 * ignores that whole layer unless the hat region contains a transparent pixel. */
export function hasVisibleLegacyHat(pixels: Uint8ClampedArray): boolean {
  for (let index = 3; index < pixels.length; index += 4) if (pixels[index] < 128) return true;
  return false;
}

export function drawSkinFace(context: CanvasRenderingContext2D, image: HTMLImageElement,
  x = 0, y = 0, width = context.canvas.width, height = context.canvas.height): void {
  context.imageSmoothingEnabled = false;
  context.clearRect(x, y, width, height);
  context.drawImage(image, 8, 8, 8, 8, x, y, width, height);
  let hatVisible = true;
  if (image.naturalHeight === 32) {
    const layer = document.createElement("canvas");
    layer.width = 32; layer.height = 32;
    const pixels = layer.getContext("2d", { willReadFrequently: true });
    if (pixels) {
      pixels.drawImage(image, 32, 0, 32, 32, 0, 0, 32, 32);
      try { hatVisible = hasVisibleLegacyHat(pixels.getImageData(0, 0, 32, 32).data); }
      catch { hatVisible = false; } // A remote image without CORS still has a usable base face.
    }
  }
  if (hatVisible) context.drawImage(image, 40, 8, 8, 8, x, y, width, height);
}

/** Render the front faces of Minecraft's UV map into a 16 × 32 portrait. */
export function drawSkinPortrait(context: CanvasRenderingContext2D, image: HTMLImageElement, model: "classic" | "slim") {
  const modern = image.naturalHeight === 64;
  const arm = modern && model === "slim" ? 3 : 4;
  context.clearRect(0, 0, 16, 32);
  drawSkinFace(context, image, 4, 0, 8, 8);
  function part(u: number, v: number, width: number, x: number, y: number) {
    context.drawImage(image, u, v, width, 12, x, y, width, 12);
  }
  part(20, 20, 8, 4, 8); // Torso
  part(44, 20, arm, 4 - arm, 8); // Right arm
  part(4, 20, 4, 4, 20); // Right leg
  if (modern) {
    part(36, 52, arm, 12, 8); // Left arm
    part(20, 52, 4, 8, 20); // Left leg
    part(20, 36, 8, 4, 8); // Jacket
    part(44, 36, arm, 4 - arm, 8);
    part(52, 52, arm, 12, 8);
    part(4, 36, 4, 4, 20);
    part(4, 52, 4, 8, 20);
  } else {
    // The legacy texture contains only right limbs. Mirror them for the left side.
    context.save();
    context.translate(16, 0);
    context.scale(-1, 1);
    part(44, 20, 4, 0, 8);
    part(4, 20, 4, 4, 20);
    context.restore();
  }
}
