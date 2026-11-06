import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent } from "react";
import { t } from "../app/i18n";

const MIN = 330;
const HANDLE = 8;
const defaultWidth = () =>
  window.innerWidth <= 1050
    ? MIN
    : Math.min(480, Math.max(360, window.innerWidth * 0.31));

export function useAgentPaneWidth(
  initial: number | undefined,
  visible: boolean,
) {
  const dock = useRef<HTMLElement>(null);
  const [preferred, setPreferred] = useState<number | undefined>(
    typeof initial === "number" && Number.isFinite(initial)
      ? initial
      : undefined,
  );
  const [limits, setLimits] = useState({ max: MIN, default: defaultWidth() });
  const [dragging, setDragging] = useState(false);
  const drag = useRef<{ id: number; x: number; width: number } | null>(null);
  const width = Math.round(
    Math.max(MIN, Math.min(limits.max, preferred ?? limits.default)),
  );
  useLayoutEffect(() => {
    const app = dock.current?.parentElement;
    if (!visible || !app) return;
    const sidebar = app.querySelector<HTMLElement>(".side");
    const main = app.querySelector<HTMLElement>(".main");
    const measure = () => {
      const reserve = main
        ? parseFloat(getComputedStyle(main).minWidth) || 500
        : 500;
      const max = Math.max(
        MIN,
        Math.floor(
          app.clientWidth - (sidebar?.offsetWidth || 0) - reserve - HANDLE,
        ),
      );
      setLimits({ max, default: defaultWidth() });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(app);
    if (sidebar) observer.observe(sidebar);
    return () => observer.disconnect();
  }, [visible]);
  const finish = () => {
    drag.current = null;
    setDragging(false);
  };
  useEffect(() => {
    window.addEventListener("blur", finish);
    return () => window.removeEventListener("blur", finish);
  }, []);
  useEffect(() => {
    if (!visible) finish();
  }, [visible]);
  const change = (value: number) =>
    setPreferred(Math.round(Math.max(MIN, Math.min(limits.max, value))));
  return {
    dock,
    width,
    preferred,
    dragging,
    handle: {
      role: "separator" as const,
      tabIndex: 0,
      "aria-label": t("Resize Agent panel"),
      "aria-orientation": "vertical" as const,
      "aria-valuemin": MIN,
      "aria-valuemax": limits.max,
      "aria-valuenow": width,
      "aria-valuetext": t("{n} px", { n: width }),
      title: t("Drag to resize · arrow keys to fine-tune · double-click to reset"),
      onPointerDown: (event: PointerEvent<HTMLDivElement>) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.focus();
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = { id: event.pointerId, x: event.clientX, width };
        setDragging(true);
      },
      onPointerMove: (event: PointerEvent<HTMLDivElement>) => {
        const current = drag.current;
        if (current?.id === event.pointerId)
          change(current.width + current.x - event.clientX);
      },
      onPointerUp: finish,
      onPointerCancel: finish,
      onLostPointerCapture: finish,
      onDoubleClick: () => setPreferred(undefined),
      onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => {
        if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
          return;
        event.preventDefault();
        change(
          event.key === "Home"
            ? MIN
            : event.key === "End"
              ? limits.max
              : width + (event.key === "ArrowLeft" ? 24 : -24),
        );
      },
    },
  };
}
