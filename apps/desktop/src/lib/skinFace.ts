/** Legacy 64x32 PNGs often fill the unused hat area with opaque black. Minecraft
 * ignores that whole layer unless the hat region contains a transparent pixel. */
export function hasVisibleLegacyHat(pixels: Uint8ClampedArray): boolean {
  for (let index = 3; index < pixels.length; index += 4) if (pixels[index] < 128) return true;
  return false;
}

export function drawSkinFace(context: CanvasRenderingContext2D, image: HTMLImageElement): void {
  const { width, height } = context.canvas;
  context.imageSmoothingEnabled = false;
  context.clearRect(0, 0, width, height);
  context.drawImage(image, 8, 8, 8, 8, 0, 0, width, height);
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
  if (hatVisible) context.drawImage(image, 40, 8, 8, 8, 0, 0, width, height);
}
