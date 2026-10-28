export const TRAY_IMAGE_SIZE = 64;
const svgNamespace = 'http://www.w3.org/2000/svg';

export function trayPercent(value: number | undefined): number | undefined {
  return value === undefined || !Number.isFinite(value) ? undefined : Math.round(Math.max(0, Math.min(100, value)));
}

/** 从主圆环复制几何和渐变，仅把网页 CSS 固化为独立 SVG 的表现属性。 */
export function serializeTraySvg(source: SVGSVGElement, value: number | undefined, stale: boolean): string {
  const clone = source.cloneNode(true) as SVGSVGElement;
  clone.setAttribute('xmlns', svgNamespace);
  clone.setAttribute('width', String(TRAY_IMAGE_SIZE));
  clone.setAttribute('height', String(TRAY_IMAGE_SIZE));
  for (const className of ['ring-track', 'ring-value']) {
    const original = source.querySelector<SVGCircleElement>(`.${className}`);
    const circle = clone.querySelector<SVGCircleElement>(`.${className}`);
    if (!original || !circle) continue;
    const style = getComputedStyle(original);
    for (const property of ['fill', 'stroke-linecap', 'stroke-linejoin']) circle.setAttribute(property, style.getPropertyValue(property));
    circle.setAttribute('stroke-width', String(Math.max(12, parseFloat(style.strokeWidth) * 2)));
    // value 的 stroke 保留 url(#id)，避免 computedStyle 将渐变写成网页绝对地址。
    if (className === 'ring-track') circle.setAttribute('stroke', style.stroke);
    if (className === 'ring-value') circle.setAttribute('opacity', stale ? '.55' : '1');
  }
  const text = document.createElementNS(svgNamespace, 'text');
  const rounded = trayPercent(value);
  text.textContent = rounded === undefined ? '—' : String(rounded);
  text.setAttribute('x', '84');
  text.setAttribute('y', '88');
  text.setAttribute('text-anchor', 'middle');
  text.setAttribute('dominant-baseline', 'central');
  text.setAttribute('font-family', 'Arial, sans-serif');
  text.setAttribute('font-size', rounded === 100 ? '70' : '82');
  text.setAttribute('font-weight', '700');
  text.setAttribute('fill', '#365b45');
  clone.append(text);
  return new XMLSerializer().serializeToString(clone);
}

function abortError(): DOMException {
  return new DOMException('托盘图像渲染已取消', 'AbortError');
}

/** 异步资源始终释放；取消后的旧快照不能覆盖新托盘状态。 */
export async function renderTrayImage(source: SVGSVGElement, value: number | undefined, stale: boolean, signal?: AbortSignal): Promise<Uint8ClampedArray> {
  if (signal?.aborted) throw abortError();
  await document.fonts.load('700 82px Arial');
  if (signal?.aborted) throw abortError();
  const url = URL.createObjectURL(new Blob([serializeTraySvg(source, value, stale)], { type: 'image/svg+xml;charset=utf-8' }));
  const image = new Image();
  try {
    await new Promise<void>((resolve, reject) => {
      const abort = () => { image.src = ''; reject(abortError()); };
      image.onload = () => { signal?.removeEventListener('abort', abort); resolve(); };
      image.onerror = () => { signal?.removeEventListener('abort', abort); reject(new Error('无法渲染托盘额度图像。')); };
      signal?.addEventListener('abort', abort, { once: true });
      image.src = url;
    });
    if (signal?.aborted) throw abortError();
    const canvas = document.createElement('canvas');
    canvas.width = TRAY_IMAGE_SIZE;
    canvas.height = TRAY_IMAGE_SIZE;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) throw new Error('当前图形环境无法生成托盘图像。');
    context.drawImage(image, 0, 0, TRAY_IMAGE_SIZE, TRAY_IMAGE_SIZE);
    return context.getImageData(0, 0, TRAY_IMAGE_SIZE, TRAY_IMAGE_SIZE).data;
  } finally {
    image.onload = null;
    image.onerror = null;
    URL.revokeObjectURL(url);
  }
}
