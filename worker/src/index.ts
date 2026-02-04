import {
  APIInteraction,
  APIInteractionResponse,
  APIInteractionResponseDeferredChannelMessageWithSource,
  APIInteractionResponsePong,
  InteractionResponseType,
  APIChatInputApplicationCommandInteraction,
  ApplicationCommandType,
  InteractionType,
  MessageFlags,
  Routes,
} from "discord-api-types/v10"
import initWasm, { render_gif, render_png, template_info } from "../pkg/maple.js"
import wasmModule from "../pkg/maple_bg.wasm"

// Load WASM once per isolate
const wasmReady = initWasm(wasmModule)

// Default template allowlist (can be overridden by env.MAPLE_TEMPLATES JSON string)
const DEFAULT_TEMPLATES = [
  "back-tattoo",
  "billboard-cityscape",
  "book",
  "toaster",
  "valentine",
  "heart-locket",
] as const

const MAX_DEFAULT_BYTES = 24_000_000 // 24MB Discord upload limit for bots

export interface Env {
  DISCORD_PUBLIC_KEY: string
  APPLICATION_ID: string
  MAPLE_TEMPLATES?: string
  MAX_OUTPUT_BYTES?: string
  ASSETS: Fetcher
}

type DiscordAttachment = {
  id: string
  url: string
  proxy_url?: string
  content_type?: string
  size?: number
  filename: string
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    if (request.method !== "POST") {
      return json({ error: "Method not allowed" }, 405)
    }

    const rawBody = await request.text()

    if (!rawBody.length) {
      return json({ error: "Empty body" }, 400)
    }

    const valid = await verifyDiscordRequest(request, env, rawBody)
    if (!valid) {
      return json({ error: "invalid request signature" }, 401)
    }

    let interaction: APIInteraction
    try {
      interaction = JSON.parse(rawBody)
    } catch (err) {
      return json({ error: "Invalid JSON" }, 400)
    }

    if (interaction.type === InteractionType.Ping) {
      const pong: APIInteractionResponsePong = { type: InteractionResponseType.Pong }
      return json(pong)
    }

    if (interaction.type !== InteractionType.ApplicationCommand) {
      return json({ error: "Unsupported interaction" }, 400)
    }

    if (interaction.data?.name !== "maple") {
      return json({ error: "Unknown command" }, 400)
    }

    if (interaction.data.type !== ApplicationCommandType.ChatInput) {
      return json({ error: "Unsupported command type" }, 400)
    }

    const optionMap = flattenOptions(interaction.data.options)
    const allowedTemplates = parseTemplateAllowlist(env)
    const template = optionMap.get("template") ?? allowedTemplates[0]

    if (!allowedTemplates.includes(template)) {
      const resp: APIInteractionResponse = {
        type: InteractionResponseType.ChannelMessageWithSource,
        data: {
          content: `Template \"${template}\" not available. Allowed: ${allowedTemplates.join(", ")}`,
          flags: MessageFlags.Ephemeral,
        },
      }
      return json(resp)
    }

    const mode = (optionMap.get("mode") ?? "image") as "image" | "text"
    const text = optionMap.get("text") ?? ""
    const autoZoom = Boolean(optionMap.get("auto_zoom"))
    const width = optionMap.has("width") ? Number(optionMap.get("width")) : undefined
    const height = optionMap.has("height") ? Number(optionMap.get("height")) : undefined

    const attachmentOption = optionMap.get("attachment") as string | undefined
  const attachment =
    attachmentOption && interaction.data.resolved?.attachments
      ? interaction.data.resolved.attachments[attachmentOption]
      : undefined

    // immediate deferred ephemeral reply
    const ack: APIInteractionResponseDeferredChannelMessageWithSource = {
      type: InteractionResponseType.DeferredChannelMessageWithSource,
      data: { flags: MessageFlags.Ephemeral },
    }
    const ackResp = json(ack)

    ctx.waitUntil(
      handleRenderAndReply({
        interaction: interaction as APIChatInputApplicationCommandInteraction,
        template,
        mode,
        text,
        autoZoom,
        width,
        height,
        attachment,
        env,
      })
    )

    return ackResp
  },
}

async function handleRenderAndReply(params: {
  interaction: APIChatInputApplicationCommandInteraction
  template: string
  mode: "image" | "text"
  text: string
  autoZoom: boolean
  width?: number
  height?: number
  attachment?: DiscordAttachment
  env: Env
}) {
  const { interaction, env, template, mode, text, autoZoom, width, height, attachment } = params
  const maxBytes = env.MAX_OUTPUT_BYTES ? Number(env.MAX_OUTPUT_BYTES) : MAX_DEFAULT_BYTES

  try {
    await wasmReady
    const templateBytes = await fetchTemplate(env, template)

    const inputs = [] as any[]

    if (mode === "text" || !attachment) {
      const safeText = text?.trim() || "Maple"
      inputs.push({
        kind: "text",
        layer: 1,
        text: safeText,
        font_size: 72,
      })
    }

    if (mode === "image" && attachment) {
      const imgBytes = await fetchAttachmentBytes(attachment)
      inputs.push({ kind: "image", layer: 1, bytes: imgBytes })
    }

    if (!inputs.length) {
      throw new Error("No inputs provided")
    }

    const options = {
      auto_zoom: autoZoom,
      width: width && width > 0 ? width : undefined,
      height: height && height > 0 ? height : undefined,
    }

    const isAnimated = await templateIsAnimated(templateBytes)
    const rendered = isAnimated
      ? render_gif(templateBytes, inputs, options)
      : render_png(templateBytes, inputs, options)

    if (rendered.length > maxBytes) {
      throw new Error(`Output exceeds limit (${Math.round(rendered.length / 1024 / 1024)}MB > ${Math.round(maxBytes / 1024 / 1024)}MB)`) 
    }

    const filename = isAnimated ? `${template}.gif` : `${template}.png`
    const description = `${template} via Maple`

    await postFollowup(env, interaction, {
      content: `Here you go! Template **${template}** (${mode}).`,
      file: new File([rendered.buffer.slice(rendered.byteOffset, rendered.byteOffset + rendered.byteLength) as ArrayBuffer], filename, {
        type: isAnimated ? "image/gif" : "image/png",
      }),
      description,
    })
  } catch (err: any) {
    console.error("render error", err)
    await postFollowup(env, interaction, {
      content: `⚠️ Failed to render: ${err?.message ?? String(err)}`,
    })
  }
}

async function verifyDiscordRequest(request: Request, env: Env, rawBody: string): Promise<boolean> {
  const signature = request.headers.get("X-Signature-Ed25519")
  const timestamp = request.headers.get("X-Signature-Timestamp")
  if (!signature || !timestamp) return false

  const publicKey = env.DISCORD_PUBLIC_KEY
  if (!publicKey) return false

  const encoder = new TextEncoder()
  const message = encoder.encode(timestamp + rawBody)

  try {
    const publicKeyBytes = hexToUint8Array(publicKey)
    const signatureBytes = hexToUint8Array(signature)
    const key = await crypto.subtle.importKey(
      "raw",
      publicKeyBytes.buffer as ArrayBuffer,
      { name: "Ed25519" },
      false,
      ["verify"]
    )
    return await crypto.subtle.verify({ name: "Ed25519" }, key, signatureBytes.buffer as ArrayBuffer, message)
  } catch (err) {
    console.error("verification error", err)
    return false
  }
}

async function fetchTemplate(env: Env, template: string): Promise<Uint8Array> {
  const url = new URL(`/templates/${template}.zip`, "https://asset.invalid")
  const res = await env.ASSETS.fetch(url)
  if (!res.ok) {
    throw new Error(`Template ${template} not found (${res.status})`)
  }
  const buf = new Uint8Array(await res.arrayBuffer())
  if (buf.length < 4 || buf[0] !== 0x50 || buf[1] !== 0x4b) {
    console.error(
      `Template fetch looked wrong`,
      JSON.stringify({
        template,
        status: res.status,
        len: buf.length,
        contentType: res.headers.get("content-type"),
        firstBytes: Array.from(buf.slice(0, 8)),
      })
    )
    throw new Error(`Template ${template} asset is corrupted or not a zip (len=${buf.length})`)
  }
  return buf
}

async function fetchAttachmentBytes(att: DiscordAttachment): Promise<Uint8Array> {
  if (!att.url) throw new Error("Attachment missing URL")
  if (att.content_type && !att.content_type.includes("png")) {
    throw new Error("Only PNG attachments are supported")
  }
  const res = await fetch(att.url, { headers: { "User-Agent": "maple-worker/1.0" } })
  if (!res.ok) {
    throw new Error(`Failed to fetch attachment (${res.status})`)
  }
  const contentType = res.headers.get("content-type")
  if (contentType && !contentType.includes("png")) {
    throw new Error("Only PNG attachments are supported")
  }
  const buf = await res.arrayBuffer()
  return new Uint8Array(buf)
}

async function templateIsAnimated(templateZip: Uint8Array): Promise<boolean> {
  // Try reading template info; fallback to assuming animation if unavailable
  try {
    const info: any = template_info(templateZip)
    return info.frames && info.frames > 1
  } catch {
    return true
  }
}

async function postFollowup(
  env: Env,
  interaction: APIInteraction,
  message: { content: string; file?: File; description?: string }
) {
  const webhook = `https://discord.com/api/v10/webhooks/${env.APPLICATION_ID}/${interaction.token}`

  if (message.file) {
    const form = new FormData()
    const attachments = [
      {
        id: 0,
        filename: message.file.name,
        description: message.description ?? "",
      },
    ]
    // Discord expects files[n] keys to align with attachments ids
    form.append("files[0]", message.file)
    form.append(
      "payload_json",
      JSON.stringify({
        content: message.content,
        flags: MessageFlags.Ephemeral,
        attachments,
      })
    )

    const res = await fetch(webhook, {
      method: "POST",
      body: form,
    })
    if (!res.ok) {
      const txt = await res.text()
      console.error("follow-up failed", res.status, txt)
    }
    return
  }

  await fetch(webhook, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      content: message.content,
      flags: MessageFlags.Ephemeral,
    }),
  })
}

function flattenOptions(options?: Array<{ name: string; value?: any; options?: any }>) {
  const map = new Map<string, any>()
  if (!options) return map
  for (const opt of options) {
    map.set(opt.name, opt.value)
    if (opt.options) {
      for (const child of opt.options) {
        map.set(child.name, child.value)
      }
    }
  }
  return map
}

function parseTemplateAllowlist(env: Env): string[] {
  if (env.MAPLE_TEMPLATES) {
    try {
      const parsed = JSON.parse(env.MAPLE_TEMPLATES)
      if (Array.isArray(parsed) && parsed.length) {
        return parsed.map(String)
      }
    } catch (err) {
      console.warn("Failed to parse MAPLE_TEMPLATES", err)
    }
  }
  return [...DEFAULT_TEMPLATES]
}

function hexToUint8Array(hex: string): Uint8Array {
  const normalized = hex.trim().toLowerCase().replace(/^0x/, "")
  if (normalized.length % 2 !== 0) {
    throw new Error("Invalid hex length")
  }
  const bytes = new Uint8Array(normalized.length / 2)
  for (let i = 0; i < normalized.length; i += 2) {
    bytes[i / 2] = parseInt(normalized.substr(i, 2), 16)
  }
  return bytes
}

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "content-type": "application/json" },
  })
}
