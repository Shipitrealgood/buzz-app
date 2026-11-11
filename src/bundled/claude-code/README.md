# Claude

Create agents that run Claude on your computer and answer you in Buzz. Each agent works in its own **Workspace** folder and can run commands and edit files on its own.

## Getting started

1. Go to **Settings** → **Agent setup** → **Harness defaults** → **Claude** to install Claude and paste an Anthropic API key from [console.anthropic.com](https://console.anthropic.com/settings/keys). Buzz keeps it in the OS credential store (Keychain on macOS), and native code gives it to every Claude process, security reviews included.
2. Choose **New agent** → **Create a new agent**, and pick **Claude** as the **Harness**.
3. Mention your agent in a conversation to give it work.

Open **Settings** → **Claude** to manage the saved key. Focus or click the masked field, or use its chevron, to open **Edit / Copy**. **Edit** shows the existing key with inline **Cancel / Save** buttons; both return to the masked view. Copy does not reveal the key. The trash button (**Remove key**) deletes it after confirmation. Key management also works when Claude needs installation or an update.

Removing a key does not revoke it at Anthropic. New local Claude sessions and security reviews need a key again; a turn already running can finish. Agents on another computer use that computer's credentials.
