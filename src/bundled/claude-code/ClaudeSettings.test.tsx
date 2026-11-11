// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Spawn } from "./claude";
import { ClaudeSettings } from "./ClaudeSettings";
import { ClaudeSetup } from "./setup";

const native = vi.hoisted(() => ({
  saved: true,
  value: "sk-ant-saved",
  invoke: vi.fn(),
}));
const notify = vi.hoisted(() => vi.fn());
const clipboard = vi.hoisted(() => vi.fn());
vi.mock("../../shared/clipboard", () => ({ writeClipboardText: clipboard }));
vi.mock("../../shared/design-system/ui/Toast", () => ({
  useToastNotification: () => notify,
}));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke: (...args: unknown[]) => native.invoke(...args),
}));
beforeEach(() => {
  notify.mockClear();
  clipboard.mockReset().mockResolvedValue(undefined);
  native.saved = true;
  native.value = "sk-ant-saved";
  native.invoke.mockReset().mockImplementation(async (command, args) => {
    if (command === "claude_api_key_status") return native.saved;
    if (command === "claude_api_key_reveal")
      return native.saved ? native.value : null;
    if (command === "claude_api_key_set") {
      if (!args.value.startsWith("sk-ant-"))
        throw "That is not an Anthropic API key.";
      native.saved = true;
      native.value = args.value;
    } else if (command === "claude_api_key_clear") native.saved = false;
    else throw new Error(`Unexpected command: ${command}`);
    return null;
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function fixture(active = () => true) {
  // No installed runtime: management must still work, without starting Claude.
  const spawn: Spawn = vi.fn(async () => ({
    write: async () => {},
    end: async () => {},
    kill: async () => {},
    exited: Promise.resolve(127),
  }));
  const setup = new ClaudeSetup(spawn);
  render(
    <StrictMode>
      <button type="button">Before settings</button>
      <ClaudeSettings setup={setup} active={active} />
      <button type="button">After settings</button>
    </StrictMode>,
  );
  return { setup, spawn };
}

async function savedKey() {
  const input = await screen.findByLabelText("Saved API key");
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "API key actions" }),
    ).toBeEnabled(),
  );
  return input;
}

async function action(name: string, user = userEvent.setup()) {
  const trigger = screen.getByRole("button", { name: "API key actions" });
  await waitFor(() => expect(trigger).toBeEnabled());
  await user.click(trigger);
  await user.click(await screen.findByRole("menuitem", { name }));
}

async function editKey(user = userEvent.setup()) {
  const input = await savedKey();
  await action("Edit", user);
  await waitFor(() => expect(input).not.toHaveAttribute("readonly"));
  return input;
}

it("adds, confirms removal, and adds again without an installed runtime", async () => {
  native.saved = false;
  const user = userEvent.setup();
  const { spawn } = fixture();
  const add = await screen.findByLabelText("Anthropic API key");
  await waitFor(() => expect(add).not.toHaveAttribute("aria-disabled"));
  expect(spawn).not.toHaveBeenCalled();
  await user.type(add, "sk-ant-first");
  await user.click(screen.getByRole("button", { name: "Save key" }));
  await savedKey();
  await user.click(screen.getByRole("button", { name: "Remove key" }));
  await user.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "Cancel",
    }),
  );
  expect(native.saved).toBe(true);
  await user.click(screen.getByRole("button", { name: "Remove key" }));
  await user.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "Remove key",
    }),
  );
  await screen.findByLabelText("Anthropic API key");
  expect(native.saved).toBe(false);
  await user.type(screen.getByLabelText("Anthropic API key"), "sk-ant-third");
  await user.click(screen.getByRole("button", { name: "Save key" }));
  await savedKey();
  expect(native.saved).toBe(true);
});

it.each(["Save", "Remove key"])(
  "keeps successful %s separate from a failed runtime refresh",
  async (operation) => {
    const user = userEvent.setup();
    const { setup } = fixture();
    let reject!: (reason: string) => void;
    const pending = new Promise<void>((_, fail) => {
      reject = fail;
    });
    const refresh = vi.spyOn(setup, "check").mockReturnValueOnce(pending);
    try {
      if (operation === "Save") {
        const input = await editKey(user);
        fireEvent.change(input, { target: { value: "sk-ant-new" } });
        await user.click(screen.getByRole("button", { name: "Save" }));
        expect(notify).toHaveBeenCalledWith("Saved", "success");
      } else {
        await savedKey();
        await user.click(screen.getByRole("button", { name: operation }));
        await user.click(
          within(screen.getByRole("alertdialog")).getByRole("button", {
            name: operation,
          }),
        );
      }
      await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
    } finally {
      await act(async () => reject("Runtime status unavailable"));
    }
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    if (operation === "Save") {
      expect(native.value).toBe("sk-ant-new");
      expect(screen.getByLabelText("Saved API key")).toHaveAttribute(
        "readonly",
      );
    } else {
      expect(native.saved).toBe(false);
      expect(screen.getByLabelText("Anthropic API key")).toBeInTheDocument();
    }
  },
);

it("keeps a failed removal open for retry and prevents duplicate writes while pending", async () => {
  const user = userEvent.setup();
  fixture();
  await screen.findByLabelText("Saved API key");
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  native.invoke.mockImplementationOnce(async () => {
    await pending;
    throw "Keychain access was denied.";
  });
  await user.click(screen.getByRole("button", { name: "Remove key" }));
  const dialog = screen.getByRole("alertdialog");
  const remove = within(dialog).getByRole("button", { name: "Remove key" });
  try {
    await user.click(remove);
    expect(remove).toHaveAttribute("aria-disabled", "true");
    expect(
      within(dialog).getByRole("button", { name: "Cancel" }),
    ).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(dialog).toBeInTheDocument();
  } finally {
    await act(async () => release());
  }
  expect(await within(dialog).findByRole("alert")).toHaveTextContent(
    "Keychain access was denied.",
  );
  expect(native.saved).toBe(true);
  await user.click(remove);
  await screen.findByLabelText("Anthropic API key");
  expect(native.saved).toBe(false);
});

it("preserves a rejected edit and blocks other key actions until saving finishes", async () => {
  const user = userEvent.setup();
  fixture();
  const input = await editKey(user);
  fireEvent.change(input, { target: { value: "invalid-key" } });
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  native.invoke.mockImplementationOnce(async () => {
    await pending;
    throw "That is not an Anthropic API key.";
  });
  try {
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByRole("button", { name: "Remove key" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "API key actions" }),
    ).not.toBeInTheDocument();
    expect(input).toHaveAttribute("readonly");
  } finally {
    await act(async () => release());
  }
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "That is not an Anthropic API key.",
  );
  expect(input).toHaveValue("invalid-key");
  expect(input).not.toHaveAttribute("readonly");
  expect(native.value).toBe("sk-ant-saved");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(input).not.toHaveValue("sk-ant-saved");
  expect(input).toHaveAttribute("readonly");
  expect(input).toHaveFocus();
  expect(
    screen.queryByRole("button", { name: "Save" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Remove key" })).toBeEnabled();
});

it("treats a credential read failure as unknown and offers retry", async () => {
  native.invoke.mockRejectedValueOnce("Secure storage is unavailable");
  // StrictMode starts two reads; both must fail to exercise the current read.
  native.invoke.mockRejectedValueOnce("Secure storage is unavailable");
  fixture();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Secure storage is unavailable",
  );
  expect(
    screen.queryByRole("button", { name: "Remove key" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save key" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  await screen.findByLabelText("Saved API key");
});

it("rejects writes from a retired settings visit", async () => {
  let active = true;
  const user = userEvent.setup();
  fixture(() => active);
  const input = await editKey(user);
  fireEvent.change(input, { target: { value: "sk-ant-new" } });
  active = false;
  fireEvent.keyDown(input, { key: "Enter" });
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(
    native.invoke.mock.calls.some(
      ([command]) =>
        command === "claude_api_key_set" || command === "claude_api_key_clear",
    ),
  ).toBe(false);
  expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
});

it("ignores an older credential read that resolves after removal", async () => {
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  const { ClaudeAuth } = await import("./auth");
  vi.spyOn(ClaudeAuth.prototype, "check").mockImplementationOnce(async () => {
    await pending;
    return {};
  });
  const user = userEvent.setup();
  fixture();
  try {
    await screen.findByLabelText("Saved API key");
    await user.click(screen.getByRole("button", { name: "Remove key" }));
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", {
        name: "Remove key",
      }),
    );
    await screen.findByLabelText("Anthropic API key");
  } finally {
    await act(async () => release());
  }
  expect(screen.getByLabelText("Anthropic API key")).toBeInTheDocument();
  expect(native.saved).toBe(false);
});

it("checks visit ownership again before confirming removal", async () => {
  let active = true;
  const user = userEvent.setup();
  fixture(() => active);
  await screen.findByLabelText("Saved API key");
  await user.click(screen.getByRole("button", { name: "Remove key" }));
  active = false;
  await user.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "Remove key",
    }),
  );
  expect(native.saved).toBe(true);
  expect(
    native.invoke.mock.calls.some(
      ([command]) => command === "claude_api_key_clear",
    ),
  ).toBe(false);
});

it("starts read-only and masked, with Edit and Copy as its only menu actions", async () => {
  const user = userEvent.setup();
  fixture();
  const input = await savedKey();
  expect(input).toHaveAttribute("readonly");
  expect(input).toHaveAttribute("type", "password");
  expect(input).not.toHaveValue(native.value);
  expect(
    native.invoke.mock.calls.every(
      ([command]) => command === "claude_api_key_status",
    ),
  ).toBe(true);
  fireEvent.change(input, { target: { value: "sk-ant-unwanted-edit" } });
  expect(input).not.toHaveValue("sk-ant-unwanted-edit");
  screen.getByRole("button", { name: "API key actions" }).focus();
  await user.keyboard("{Enter}");
  await screen.findByRole("menuitem", { name: "Edit" });
  expect(
    screen.getAllByRole("menuitem").map((item) => item.textContent),
  ).toEqual(["Edit", "Copy"]);
});

it("opens actions from pointer or keyboard input focus without reopening on dismissal or Edit", async () => {
  const user = userEvent.setup();
  fixture();
  const input = await savedKey();
  await user.click(input);
  await screen.findByRole("menuitem", { name: "Edit" });
  await user.keyboard("{Escape}");
  await waitFor(() =>
    expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
  );
  const trigger = screen.getByRole("button", { name: "API key actions" });
  trigger.focus();
  expect(trigger).toHaveFocus();
  await user.tab({ shift: true });
  expect(input).toHaveFocus();
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  await user.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Before settings" })).toHaveFocus();
  await user.tab();
  await waitFor(() =>
    expect(screen.getByRole("menuitem", { name: "Edit" })).toHaveFocus(),
  );
  await user.keyboard("{Enter}");
  await waitFor(() => expect(input).toHaveValue(native.value));
  expect(input).toHaveFocus();
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(input).not.toHaveValue(native.value);
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  await user.click(input);
  await screen.findByRole("menuitem", { name: "Copy" });
});

it("offers retry through the menu after denied Edit", async () => {
  const user = userEvent.setup();
  fixture();
  const input = await savedKey();
  native.invoke.mockRejectedValueOnce("Keychain access was denied.");
  await action("Edit", user);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Keychain access was denied.",
  );
  expect(input).not.toHaveValue(native.value);
  expect(input).toHaveAttribute("readonly");
  await editKey(user);
  expect(input).toHaveValue(native.value);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it.each([
  ["Edit", "removal"],
  ["Copy", "removal"],
  ["Edit", "retirement"],
  ["Edit", "closing"],
])("discards pending %s after %s", async (operation, exit) => {
  let active = true;
  const user = userEvent.setup();
  const { setup } = fixture(() => active);
  await savedKey();
  let release!: (value: string) => void;
  const pending = new Promise<string>((resolve) => {
    release = resolve;
  });
  const read = vi.spyOn(setup.auth, "reveal").mockReturnValueOnce(pending);
  try {
    await action(operation, user);
    await waitFor(() => expect(read).toHaveBeenCalledOnce());
    expect(screen.getByLabelText("Saved API key")).toHaveAttribute("readonly");
    if (exit === "removal")
      await user.click(screen.getByRole("button", { name: "Remove key" }));
    else if (exit === "retirement") active = false;
    else cleanup();
  } finally {
    await act(async () => release("sk-ant-late"));
  }
  if (exit === "closing")
    render(<ClaudeSettings setup={setup} active={() => true} />);
  const input = await screen.findByLabelText("Saved API key");
  expect(input).not.toHaveValue("sk-ant-late");
  expect(input).toHaveAttribute("readonly");
  expect(clipboard).not.toHaveBeenCalled();
  expect(notify).not.toHaveBeenCalled();
  if (exit === "removal")
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", {
        name: "Cancel",
      }),
    );
  expect(input).toHaveAttribute("type", "password");
  expect(input).not.toHaveValue("sk-ant-late");
});

it("discards edit values and drafts when credentials change elsewhere", async () => {
  const user = userEvent.setup();
  const { setup } = fixture();
  const input = await editKey(user);
  fireEvent.change(input, { target: { value: "sk-ant-draft" } });
  await act(async () => {
    await setup.auth.save("sk-ant-other");
  });
  expect(input).not.toHaveValue("sk-ant-saved");
  expect(input).not.toHaveValue("sk-ant-other");
  expect(input).not.toHaveValue("sk-ant-draft");
  expect(input).toHaveAttribute("readonly");
  expect(input).toHaveAttribute("type", "password");
  expect(
    screen.queryByRole("button", { name: "Save" }),
  ).not.toBeInTheDocument();
});

it("returns to add-key setup when a requested key no longer exists", async () => {
  const user = userEvent.setup();
  fixture();
  await savedKey();
  native.saved = false;
  await action("Edit", user);
  await screen.findByLabelText("Anthropic API key");
  expect(screen.queryByLabelText("Saved API key")).not.toBeInTheDocument();
});

it("does not carry an edited key into a reopened page or fetch it at mount", async () => {
  const user = userEvent.setup();
  const { setup } = fixture();
  await editKey(user);
  const reads = native.invoke.mock.calls.filter(
    ([command]) => command === "claude_api_key_reveal",
  ).length;
  cleanup();
  render(<ClaudeSettings setup={setup} active={() => true} />);
  const input = await savedKey();
  expect(input).not.toHaveValue(native.value);
  expect(input).toHaveAttribute("readonly");
  expect(
    native.invoke.mock.calls.filter(
      ([command]) => command === "claude_api_key_reveal",
    ),
  ).toHaveLength(reads);
});

it("edits the saved key, gates Save, and discards the draft on Cancel", async () => {
  const user = userEvent.setup();
  fixture();
  const input = await editKey(user);
  expect(input).toHaveValue(native.value);
  expect(input).toHaveAttribute("type", "text");
  await waitFor(() => expect(input).toHaveFocus());
  const save = screen.getByRole("button", { name: "Save" });
  const remove = screen.getByRole("button", { name: "Remove key" });
  expect(remove).toBeDisabled();
  await user.click(remove);
  expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "API key actions" }),
  ).not.toBeInTheDocument();
  expect(save).toBeDisabled();
  await user.keyboard("{Enter}");
  fireEvent.change(input, { target: { value: "" } });
  expect(save).toBeDisabled();
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.change(input, { target: { value: "sk-ant-draft" } });
  expect(save).toBeEnabled();
  fireEvent.change(input, { target: { value: native.value } });
  expect(save).toBeDisabled();
  fireEvent.change(input, { target: { value: "sk-ant-draft" } });
  expect(native.value).toBe("sk-ant-saved");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(input).toHaveAttribute("readonly");
  expect(input).not.toHaveValue("sk-ant-draft");
  expect(input).not.toHaveValue(native.value);
  expect(save).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "API key actions" })).toBeEnabled();
  expect(remove).toBeEnabled();
  expect(
    native.invoke.mock.calls.some(
      ([command]) => command === "claude_api_key_set",
    ),
  ).toBe(false);
});

it("saves a replacement with Enter after composition ends", async () => {
  const user = userEvent.setup();
  fixture();
  const input = await editKey(user);
  fireEvent.change(input, { target: { value: "sk-ant-new" } });
  fireEvent.keyDown(input, { key: "Enter", isComposing: true });
  expect(
    native.invoke.mock.calls.some(
      ([command]) => command === "claude_api_key_set",
    ),
  ).toBe(false);
  await user.keyboard("{Enter}");
  await waitFor(() =>
    expect(
      screen.queryByRole("button", { name: "Save" }),
    ).not.toBeInTheDocument(),
  );
  await waitFor(() => expect(input).toHaveFocus());
  expect(native.value).toBe("sk-ant-new");
  expect(input).toHaveAttribute("type", "password");
  expect(input).toHaveAttribute("readonly");
});

it("retries clipboard failures and copies the current key without revealing it", async () => {
  const user = userEvent.setup();
  fixture();
  const input = await savedKey();
  clipboard.mockRejectedValueOnce("Clipboard unavailable");
  await action("Copy", user);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Clipboard unavailable",
  );
  expect(notify).not.toHaveBeenCalled();
  expect(input).not.toHaveValue(native.value);
  native.value = "sk-ant-current";
  await action("Copy", user);
  await waitFor(() => expect(clipboard).toHaveBeenLastCalledWith(native.value));
  expect(notify).toHaveBeenCalledWith("API key copied", "success");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(input).not.toHaveValue(native.value);
  expect(input).toHaveAttribute("readonly");
  expect(input).toHaveAttribute("type", "password");
  expect(
    native.invoke.mock.calls.some(
      ([command]) => command === "claude_api_key_set",
    ),
  ).toBe(false);
});
