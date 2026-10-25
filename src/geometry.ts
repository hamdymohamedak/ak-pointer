export interface WindowBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ScreenPoint {
  x: number;
  y: number;
}

export function toRelative(
  locked: WindowBox,
  current: WindowBox,
  screenX: number,
  screenY: number,
): ScreenPoint {
  if (current.w < 1 || current.h < 1 || locked.w < 1 || locked.h < 1) {
    return { x: screenX - current.x, y: screenY - current.y };
  }
  return {
    x: (screenX - current.x) * (locked.w / current.w),
    y: (screenY - current.y) * (locked.h / current.h),
  };
}

export function toScreen(
  recordedW: number,
  recordedH: number,
  current: WindowBox,
  relX: number,
  relY: number,
): ScreenPoint {
  const scaleX = recordedW > 1 ? current.w / recordedW : 1;
  const scaleY = recordedH > 1 ? current.h / recordedH : 1;
  return { x: current.x + relX * scaleX, y: current.y + relY * scaleY };
}

export function sizeChanged(recordedW: number, recordedH: number, current: WindowBox): boolean {
  return Math.abs(current.w - recordedW) > 2 || Math.abs(current.h - recordedH) > 2;
}
