/*
Registers/updates the /maple command.
Usage (global):
  APPLICATION_ID=xxx BOT_TOKEN=xxx bun run scripts/register-command.ts
Optional: GUILD_ID=xxx for quicker iteration (guild command).
*/

import {
  ApplicationCommandOptionType,
  ApplicationCommandType,
  RESTPostAPIApplicationCommandsJSONBody,
  Routes,
} from "discord-api-types/v10"

const { APPLICATION_ID, BOT_TOKEN, GUILD_ID } = process.env

if (!APPLICATION_ID || !BOT_TOKEN) {
  console.error("APPLICATION_ID and BOT_TOKEN are required")
  process.exit(1)
}

const templates = [
  "back-tattoo",
  "billboard-cityscape",
  "book",
  "toaster",
  "valentine",
  "heart-locket",
]

const command: RESTPostAPIApplicationCommandsJSONBody = {
  name: "maple",
  description: "Render a GIF with Maple templates",
  type: ApplicationCommandType.ChatInput,
  options: [
    {
      type: ApplicationCommandOptionType.String,
      name: "template",
      description: "Template to use",
      required: true,
      choices: templates.slice(0, 25).map((t) => ({ name: t, value: t })),
    },
    {
      type: ApplicationCommandOptionType.String,
      name: "mode",
      description: "Use image attachment or text",
      required: false,
      choices: [
        { name: "image", value: "image" },
        { name: "text", value: "text" },
      ],
    },
    {
      type: ApplicationCommandOptionType.String,
      name: "text",
      description: "Text to render (used when mode=text)",
      required: false,
    },
    {
      type: ApplicationCommandOptionType.Attachment,
      name: "attachment",
      description: "PNG image to place into the template",
      required: false,
    },
    {
      type: ApplicationCommandOptionType.Boolean,
      name: "auto_zoom",
      description: "Auto-fit inputs to frame",
      required: false,
    },
    {
      type: ApplicationCommandOptionType.Integer,
      name: "width",
      description: "Output width (pixels)",
      required: false,
    },
    {
      type: ApplicationCommandOptionType.Integer,
      name: "height",
      description: "Output height (pixels)",
      required: false,
    },
  ],
}

const apiBase = "https://discord.com/api/v10"
const route = GUILD_ID
  ? apiBase + Routes.applicationGuildCommands(APPLICATION_ID!, GUILD_ID)
  : apiBase + Routes.applicationCommands(APPLICATION_ID!)

async function main() {
  const res = await fetch(route, {
    method: "PUT",
    headers: {
      Authorization: `Bot ${BOT_TOKEN}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify([command]),
  })

  if (!res.ok) {
    console.error("Failed to register command", res.status, await res.text())
    process.exit(1)
  }

  console.log("Registered /maple", GUILD_ID ? "(guild scope)" : "(global)")
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
