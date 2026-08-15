import { ChevronDown, Check } from "lucide-react";
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { calculateModelMenuPlacement, nextModelIndex, type ModelMenuPlacement } from "@/enterprise-model-select";

type EnterpriseModelSelectProps = {
  disabled?: boolean;
  onChange: (value: string) => void;
  options: string[];
  value: string;
};

export function EnterpriseModelSelect({ disabled = false, onChange, options, value }: EnterpriseModelSelectProps) {
  const listboxId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(-1);
  const [placement, setPlacement] = useState<ModelMenuPlacement | null>(null);

  const updatePlacement = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    setPlacement(calculateModelMenuPlacement(trigger.getBoundingClientRect(), window.innerWidth, window.innerHeight));
  }, []);

  const openMenu = useCallback(() => {
    if (disabled || options.length === 0) return;
    setActiveIndex(Math.max(0, options.indexOf(value)));
    setOpen(true);
  }, [disabled, options, value]);

  const closeMenu = useCallback((restoreFocus = false) => {
    setOpen(false);
    setPlacement(null);
    if (restoreFocus) requestAnimationFrame(() => triggerRef.current?.focus());
  }, []);

  const commitSelection = useCallback((index: number) => {
    const option = options[index];
    if (option === undefined) return;
    onChange(option);
    closeMenu(true);
  }, [closeMenu, onChange, options]);

  useLayoutEffect(() => {
    if (open) updatePlacement();
  }, [open, updatePlacement]);

  useEffect(() => {
    if (!open) return;
    const update = () => updatePlacement();
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && !triggerRef.current?.contains(target) && !listRef.current?.contains(target)) closeMenu();
    };
    window.addEventListener("resize", update);
    document.addEventListener("scroll", update, true);
    document.addEventListener("pointerdown", handlePointerDown);
    return () => {
      window.removeEventListener("resize", update);
      document.removeEventListener("scroll", update, true);
      document.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [closeMenu, open, updatePlacement]);

  useEffect(() => {
    if (!open || activeIndex < 0) return;
    listRef.current
      ?.querySelector<HTMLElement>(`[data-option-index="${activeIndex}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open]);

  useEffect(() => {
    if (disabled && open) closeMenu();
  }, [closeMenu, disabled, open]);

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      if (!open) openMenu();
      setActiveIndex((current) => nextModelIndex(
        current,
        options.length,
        event.key as "ArrowDown" | "ArrowUp" | "Home" | "End",
      ));
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) commitSelection(activeIndex);
      else openMenu();
      return;
    }
    if (event.key === "Escape" && open) {
      event.preventDefault();
      closeMenu(true);
      return;
    }
    if (event.key === "Tab" && open) closeMenu();
  };

  const menuStyle: CSSProperties | undefined = placement ? {
    bottom: placement.bottom,
    left: placement.left,
    maxHeight: placement.maxHeight,
    top: placement.top,
    width: placement.width,
  } : undefined;
  const isLight = triggerRef.current?.closest(".shell")?.classList.contains("light") === true;

  return (
    <div className="enterprise-model-select">
      <button
        aria-activedescendant={open && activeIndex >= 0 ? `${listboxId}-option-${activeIndex}` : undefined}
        aria-controls={listboxId}
        aria-expanded={open}
        aria-haspopup="listbox"
        className="enterprise-model-trigger"
        disabled={disabled}
        onClick={() => open ? closeMenu() : openMenu()}
        onKeyDown={handleKeyDown}
        ref={triggerRef}
        role="combobox"
        title={value}
        type="button"
      >
        <span>{value || "--"}</span>
        <ChevronDown aria-hidden="true" className={open ? "open" : undefined} />
      </button>
      {open && placement ? createPortal(
        <div
          aria-label="可用模型"
          className={`enterprise-model-menu ${isLight ? "light" : "dark"}`}
          id={listboxId}
          ref={listRef}
          role="listbox"
          style={menuStyle}
        >
          {options.map((option, index) => (
            <div
              aria-selected={option === value}
              className={`enterprise-model-option ${index === activeIndex ? "active" : ""}`}
              data-option-index={index}
              id={`${listboxId}-option-${index}`}
              key={option}
              onClick={() => commitSelection(index)}
              onPointerDown={(event) => event.preventDefault()}
              onPointerMove={() => setActiveIndex(index)}
              role="option"
              title={option}
            >
              <span>{option}</span>
              {option === value ? <Check aria-hidden="true" /> : null}
            </div>
          ))}
        </div>,
        document.body,
      ) : null}
    </div>
  );
}
