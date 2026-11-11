import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { writeClipboardText } from "../../shared/clipboard";
import { CaretDownIcon } from "../../shared/design-system/icons";
import { Button } from "../../shared/design-system/ui/Button";
import { Field } from "../../shared/design-system/ui/Field";
import { IconButton } from "../../shared/design-system/ui/IconButton";
import { Input } from "../../shared/design-system/ui/Input";
import { InputGroup } from "../../shared/design-system/ui/InputGroup";
import { LoadingRegion } from "../../shared/design-system/ui/LoadingRegion";
import {
  MenuItem,
  MenuPopup,
  MenuRoot,
  MenuTrigger,
} from "../../shared/design-system/ui/Menu";
import { useToastNotification } from "../../shared/design-system/ui/Toast";
import type { ClaudeSetup } from "./setup";

/** Saved credentials are read only on an explicit Edit or Copy action. */
export function SavedClaudeKey({
  setup,
  active,
  disabled,
  conceal,
  onMissing,
  onSavingChange,
  removeAction,
}: {
  setup: ClaudeSetup;
  active(): boolean;
  disabled: boolean;
  conceal: boolean;
  onMissing(): void;
  onSavingChange(saving: boolean): void;
  removeAction(editing: boolean): ReactNode;
}) {
  const { auth } = setup;
  const [value, setValue] = useState<string>();
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [menuOpen, setMenuOpen] = useState(false);
  const request = useRef(0);
  const writing = useRef(false);
  const input = useRef<HTMLInputElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const notify = useToastNotification();
  const editing = !conceal && value !== undefined;
  const changed = editing && draft.trim() !== value;
  const reset = useCallback(() => {
    request.current++;
    setValue(undefined);
    setDraft("");
    setPending(false);
    setError("");
    setMenuOpen(false);
  }, []);
  useEffect(() => {
    reset();
    if (conceal) return;
    const unsubscribe = auth.subscribe(() => {
      if (!writing.current) reset();
    });
    return () => {
      request.current++;
      unsubscribe();
    };
  }, [auth, conceal, reset]);
  useEffect(() => {
    if (editing) input.current?.focus();
  }, [editing]);
  // MenuTrigger establishes the popup anchor and keyboard focus.
  const openActions = () => {
    if (
      !menuOpen &&
      !editing &&
      active() &&
      !disabled &&
      !conceal &&
      !pending &&
      !saving
    )
      trigger.current?.click();
  };
  const access = async (copy: boolean) => {
    if (
      !active() ||
      disabled ||
      conceal ||
      pending ||
      editing ||
      writing.current
    )
      return;
    const current = ++request.current;
    setMenuOpen(false);
    setPending(true);
    setError("");
    try {
      const saved = await auth.reveal();
      if (request.current !== current || !active()) return;
      if (saved === undefined) onMissing();
      else if (copy) {
        await writeClipboardText(saved);
        if (request.current === current && active())
          notify("API key copied", "success");
      } else {
        setValue(saved);
        setDraft(saved);
      }
    } catch (failure) {
      if (request.current === current && active()) setError(String(failure));
    } finally {
      if (request.current === current) setPending(false);
    }
  };
  const save = async () => {
    if (
      !active() ||
      disabled ||
      conceal ||
      writing.current ||
      !editing ||
      !changed ||
      !draft.trim()
    )
      return;
    const current = request.current;
    const focused = document.activeElement;
    let saved = false;
    writing.current = true;
    setSaving(true);
    onSavingChange(true);
    setError("");
    try {
      await auth.save(draft.trim());
      if (!active() || request.current !== current) return;
      saved = true;
      reset();
      notify("Saved", "success");
      await setup.check();
    } catch (failure) {
      if (active()) setError(String(failure));
    } finally {
      writing.current = false;
      setSaving(false);
      onSavingChange(false);
      if (
        saved &&
        active() &&
        (document.activeElement === focused ||
          document.activeElement === document.body)
      )
        input.current?.focus();
    }
  };
  return (
    <LoadingRegion
      pending={pending}
      label="Reading saved key…"
      statusVisibility="screen-reader"
    >
      <div aria-busy={saving}>
        <Field
          label="Saved API key"
          labelVisibility="hidden"
          description="Saved securely on this computer."
          error={error}
        >
          <div className="flex min-w-0 items-center gap-3">
            <div className="min-w-0 flex-1">
              <MenuRoot
                open={menuOpen && !editing && !conceal}
                onOpenChange={setMenuOpen}
              >
                <InputGroup
                  trailing={
                    editing ? (
                      <span className="flex items-center gap-1">
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={saving}
                          onClick={() => {
                            reset();
                            input.current?.focus();
                          }}
                        >
                          Cancel
                        </Button>
                        <Button
                          size="sm"
                          variant="prominent"
                          disabled={disabled || !draft.trim() || !changed}
                          loading={saving}
                          onClick={() => void save()}
                        >
                          Save
                        </Button>
                      </span>
                    ) : (
                      <MenuTrigger
                        render={
                          <IconButton
                            ref={trigger}
                            aria-label="API key actions"
                            title="API key actions"
                            icon={
                              <CaretDownIcon size={16} aria-hidden="true" />
                            }
                            disabled={disabled || conceal || saving}
                            loading={pending}
                          />
                        }
                      />
                    )
                  }
                >
                  <Input
                    ref={input}
                    type={editing ? "text" : "password"}
                    value={editing ? draft : "•".repeat(96)}
                    readOnly={!editing || disabled || saving || conceal}
                    autoComplete="off"
                    spellCheck={false}
                    onFocus={(event) => {
                      const from = event.relatedTarget;
                      if (
                        from === trigger.current ||
                        (from instanceof Node && menu.current?.contains(from))
                      )
                        return;
                      openActions();
                    }}
                    onClick={openActions}
                    onValueChange={(next) => {
                      if (
                        !active() ||
                        disabled ||
                        saving ||
                        conceal ||
                        !editing
                      )
                        return;
                      setDraft(next);
                      setError("");
                    }}
                    onKeyDown={(event) => {
                      if (
                        event.key !== "Enter" ||
                        event.nativeEvent.isComposing
                      )
                        return;
                      event.preventDefault();
                      void save();
                    }}
                  />
                </InputGroup>
                <MenuPopup
                  ref={menu}
                  align="end"
                  size="compact"
                  finalFocus={() => trigger.current ?? input.current}
                >
                  <MenuItem
                    disabled={disabled || conceal || pending || saving}
                    onClick={() => void access(false)}
                  >
                    Edit
                  </MenuItem>
                  <MenuItem
                    disabled={disabled || conceal || pending || saving}
                    onClick={() => void access(true)}
                  >
                    Copy
                  </MenuItem>
                </MenuPopup>
              </MenuRoot>
            </div>
            {removeAction(editing)}
          </div>
        </Field>
      </div>
    </LoadingRegion>
  );
}
