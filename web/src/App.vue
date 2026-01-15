<template>
  <div class="min-h-screen page-shell">
    <main id="main" tabindex="-1" class="studio-main">
      <form class="studio-grid" @submit.prevent="renderImage">
        <section class="preview-panel">
          <div class="flex flex-col gap-4 h-full">
            <div class="preview-frame flex-1">
              <img v-if="previewUrl" :src="previewUrl" alt="Rendered preview" class="w-full h-full object-contain" />
              <div v-else class="text-sm text-muted">No render yet.</div>
            </div>

          </div>
        </section>

        <div
          class="resize-handle"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize sidebar"
          tabindex="0"
          @pointerdown="startResize"
          @keydown="onResizeKeydown"
        ></div>

        <aside class="card p-5 sidebar" :style="{ '--sidebar-width': `${sidebarWidth}px` }">
          <div class="flex flex-col gap-5">
            <div class="flex items-center gap-3">
              <button
                class="btn-primary tap-target flex-1 px-4 py-3 rounded-2xl text-sm font-semibold tracking-wide"
                :class="isRendering ? 'opacity-60' : ''"
                :disabled="isRendering || !wasmReady"
                type="submit"
              >
                <span v-if="!wasmReady">WASM build missing — run bun run build:wasm</span>
                <span v-else-if="isRendering">Rendering…</span>
                <span v-else>Render GIF</span>
              </button>
              <button
                class="icon-button tap-target"
                type="button"
                :disabled="!previewUrl"
                @click="downloadImage"
                aria-label="Download GIF"
              >
                <span class="i-lucide-download" aria-hidden="true"></span>
              </button>
            </div>

            <div class="divider"></div>
            <div class="flex flex-col gap-2">
              <div class="text-sm text-muted">Templates</div>
              <div class="grid gap-3">
                <button
                  v-for="template in templates"
                  :key="template.id"
                  class="template-tile tap-target text-left px-3 py-2"
                  :class="selectedTemplateId === template.id ? 'is-active' : ''"
                  type="button"
                  @click="selectTemplate(template.id)"
                >
                  <div class="text-sm font-semibold">{{ template.name }}</div>
                  <div class="text-sm text-muted">{{ template.vibe }}</div>
                </button>
              </div>
              <div class="flex items-center gap-2 text-sm text-muted">
                <span class="i-lucide-folder-open" aria-hidden="true"></span>
                {{ selectedTemplate?.file }}
              </div>
              <div v-if="templateInfo" class="text-sm text-muted">
                {{ templateInfo.width }} × {{ templateInfo.height }} · {{ templateInfo.frames }} frame<span v-if="templateInfo.frames !== 1">s</span>
              </div>
            </div>

            <div class="divider"></div>

            <div class="flex flex-col gap-4">
              <div class="flex items-center justify-between">
                <div class="text-sm font-semibold">Inputs</div>
                <div class="text-sm text-muted">{{ inputs.length }}</div>
              </div>

              <div class="flex flex-col gap-4">
                <div v-for="(input, index) in inputs" :key="input.id" class="card-soft p-3">
                  <div class="flex flex-col gap-3">
                    <div class="flex items-center justify-between">
                      <div class="text-sm font-semibold">Input {{ index + 1 }}</div>
                      <button
                        class="text-sm text-muted tap-target"
                        type="button"
                        @click="removeInput(input.id)"
                        :disabled="inputs.length === 1"
                      >
                        Remove
                      </button>
                    </div>

                    <div class="grid gap-3">
                      <div class="flex flex-col gap-1">
                        <label class="text-sm text-muted">Type</label>
                        <div class="input-shell">
                          <select v-model="input.kind" class="w-full bg-transparent text-sm" name="input-type">
                            <option value="image">PNG image</option>
                            <option value="text">Text</option>
                          </select>
                        </div>
                      </div>

                      <div class="flex flex-col gap-1">
                        <label class="text-sm text-muted">Layer</label>
                        <div class="input-shell">
                          <input
                            v-model.number="input.layer"
                            type="number"
                            min="1"
                            max="255"
                            class="w-full bg-transparent text-sm"
                            name="input-layer"
                            inputmode="numeric"
                          />
                        </div>
                      </div>
                    </div>

                    <div v-if="input.kind === 'image'" class="flex flex-col gap-2">
                      <div class="input-shell">
                        <input
                          type="file"
                          accept="image/png"
                          class="w-full text-sm"
                          @change="(event) => onFileChange(event, input)"
                        />
                      </div>
                      <div class="text-sm text-muted">PNG only. Files never leave your browser.</div>
                      <div v-if="input.file" class="text-sm text-subtle break-words">{{ input.file.name }}</div>
                    </div>

                    <div v-else class="flex flex-col gap-3">
                      <div class="input-shell">
                        <textarea
                          v-model="input.text"
                          rows="2"
                          placeholder="Enter text…"
                          class="w-full bg-transparent text-sm resize-none"
                          name="input-text"
                          @keydown="onTextareaKeydown"
                        ></textarea>
                      </div>
                      <div class="grid gap-3">
                        <div class="flex flex-col gap-1">
                          <label class="text-sm text-muted">Font size</label>
                          <div class="input-shell">
                            <input
                              v-model.number="input.fontSize"
                              type="number"
                              min="10"
                              max="240"
                              class="w-full bg-transparent text-sm"
                              name="input-font-size"
                              inputmode="numeric"
                            />
                          </div>
                        </div>
                        <div class="flex flex-col gap-1">
                          <label class="text-sm text-muted">Padding</label>
                          <div class="input-shell">
                            <input
                              v-model.number="input.padding"
                              type="number"
                              min="0"
                              max="200"
                              class="w-full bg-transparent text-sm"
                              name="input-padding"
                              inputmode="numeric"
                            />
                          </div>
                        </div>
                        <div class="flex flex-col gap-1">
                          <label class="text-sm text-muted">Text color</label>
                          <div class="input-shell">
                            <input
                              v-model="input.color"
                              type="text"
                              placeholder="#111111…"
                              class="w-full bg-transparent text-sm"
                              name="input-color"
                              spellcheck="false"
                            />
                          </div>
                        </div>
                        <div class="flex flex-col gap-1">
                          <label class="text-sm text-muted">Background</label>
                          <div class="input-shell">
                            <input
                              v-model="input.background"
                              type="text"
                              placeholder="#ffffff…"
                              class="w-full bg-transparent text-sm"
                              name="input-background"
                              spellcheck="false"
                            />
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <div class="flex items-center gap-3">
                <button
                  class="icon-button tap-target"
                  type="button"
                  @click="addInput('image')"
                  aria-label="Add image input"
                >
                  <span class="i-lucide-image" aria-hidden="true"></span>
                </button>
                <button
                  class="icon-button tap-target"
                  type="button"
                  @click="addInput('text')"
                  aria-label="Add text input"
                >
                  <span class="i-lucide-type" aria-hidden="true"></span>
                </button>
              </div>
            </div>

            <div class="divider"></div>

            <div class="flex flex-col gap-4">
              <div class="text-sm font-semibold">Render settings</div>
              <div v-if="templateInfo && templateInfo.frames > 1" class="text-sm text-muted">
                Animation · {{ templateInfo.frames }} frames
              </div>
              <div class="grid gap-3">
                <div class="flex items-center gap-2">
                  <input id="autoZoom" v-model="autoZoom" type="checkbox" class="checkbox-accent tap-target" />
                  <label for="autoZoom" class="text-sm">Auto-fit inputs</label>
                </div>
                <div class="flex flex-col gap-1">
                  <label class="text-sm text-muted">Output width</label>
                  <div class="input-shell">
                    <input
                      v-model="outputWidth"
                      type="number"
                      min="1"
                      placeholder="Auto…"
                      class="w-full bg-transparent text-sm"
                      name="output-width"
                      inputmode="numeric"
                    />
                  </div>
                </div>
                <div class="flex flex-col gap-1">
                  <label class="text-sm text-muted">Output height</label>
                  <div class="input-shell">
                    <input
                      v-model="outputHeight"
                      type="number"
                      min="1"
                      placeholder="Auto…"
                      class="w-full bg-transparent text-sm"
                      name="output-height"
                      inputmode="numeric"
                    />
                  </div>
                </div>
              </div>
            </div>

            <div class="flex flex-col gap-3" aria-live="polite">
              <div v-if="errorMessage" class="text-sm">Error: {{ errorMessage }}</div>
            </div>

            <div class="divider"></div>

            <div class="text-sm text-muted">
              <a
                href="https://github.com/taskylizard/maple"
                class="footer-link"
                target="_blank"
                rel="noreferrer"
              >
                View repository
              </a>
              <span aria-hidden="true"> · </span>
              <a
                href="https://uwu.network/~tasky"
                class="footer-link"
                target="_blank"
                rel="noreferrer"
              >
                made by taskylizard
              </a>
            </div>
          </div>
        </aside>
      </form>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { templates } from './templates'

type TemplateInfo = {
  width: number
  height: number
  frames: number
  delay: number
  hold: number
  palette: number[]
}

type InputKind = 'image' | 'text'

type InputItem = {
  id: string
  kind: InputKind
  layer: number
  file?: File
  text: string
  fontSize: number
  padding: number
  color: string
  background: string
}

const selectedTemplateId = ref(templates[0]?.id ?? '')
const templateInfo = ref<TemplateInfo | null>(null)
const autoZoom = ref(true)
const outputWidth = ref<number | ''>('')
const outputHeight = ref<number | ''>('')
const previewUrl = ref('')
const errorMessage = ref('')
const isRendering = ref(false)
const wasmReady = ref(false)

let wasmModule: any = null
const templateCache = new Map<string, Uint8Array>()

const selectedTemplate = computed(() => templates.find((item) => item.id === selectedTemplateId.value))

const makeId = () => {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return `id-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

const inputs = ref<InputItem[]>([
  {
    id: makeId(),
    kind: 'image',
    layer: 1,
    text: '',
    fontSize: 72,
    padding: 40,
    color: '#111111',
    background: '#ffffff',
  },
])

const selectTemplate = (id: string) => {
  selectedTemplateId.value = id
  loadTemplateInfo().catch(() => undefined)
}

const addInput = (kind: InputKind) => {
  inputs.value.push({
    id: makeId(),
    kind,
    layer: 1,
    text: '',
    fontSize: 72,
    padding: 40,
    color: '#111111',
    background: '#ffffff',
  })
}

const removeInput = (id: string) => {
  if (inputs.value.length === 1) return
  inputs.value = inputs.value.filter((input) => input.id !== id)
}

const onFileChange = (event: Event, input: InputItem) => {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return
  if (file.type && file.type !== 'image/png') {
    errorMessage.value = 'Only PNG files are allowed.'
    target.value = ''
    return
  }
  input.file = file
}

const onTextareaKeydown = (event: KeyboardEvent) => {
  if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
    event.preventDefault()
    renderImage()
  }
}

const sidebarWidth = ref(320)
const clampSidebarWidth = (value: number) => {
  const minWidth = 240
  const maxWidth = Math.max(260, Math.floor(window.innerWidth * 0.35))
  return Math.min(Math.max(value, minWidth), maxWidth)
}

const startResize = (event: PointerEvent) => {
  const startX = event.clientX
  const startWidth = sidebarWidth.value
  const handleMove = (moveEvent: PointerEvent) => {
    const delta = startX - moveEvent.clientX
    sidebarWidth.value = clampSidebarWidth(startWidth + delta)
  }
  const stopResize = () => {
    window.removeEventListener('pointermove', handleMove)
    window.removeEventListener('pointerup', stopResize)
  }
  window.addEventListener('pointermove', handleMove)
  window.addEventListener('pointerup', stopResize)
}

const onResizeKeydown = (event: KeyboardEvent) => {
  if (event.key === 'ArrowLeft') {
    event.preventDefault()
    sidebarWidth.value = clampSidebarWidth(sidebarWidth.value + 16)
  }
  if (event.key === 'ArrowRight') {
    event.preventDefault()
    sidebarWidth.value = clampSidebarWidth(sidebarWidth.value - 16)
  }
  if (event.key === 'Home') {
    event.preventDefault()
    sidebarWidth.value = clampSidebarWidth(240)
  }
  if (event.key === 'End') {
    event.preventDefault()
    sidebarWidth.value = clampSidebarWidth(Math.floor(window.innerWidth * 0.35))
  }
}

const initWasm = async () => {
  try {
    const module = await import('./wasm/maple.js')
    await module.default()
    wasmModule = module
    wasmReady.value = true
  } catch (error) {
    wasmReady.value = false
    errorMessage.value = 'WASM not found. Run bun run build:wasm.'
  }
}

const fetchTemplateZip = async () => {
  const template = selectedTemplate.value
  if (!template) throw new Error('No template selected.')
  if (templateCache.has(template.id)) return templateCache.get(template.id) as Uint8Array
  const response = await fetch(`${import.meta.env.BASE_URL}templates/${template.file}`)
  if (!response.ok) throw new Error('Failed to load template.')
  const buffer = await response.arrayBuffer()
  const bytes = new Uint8Array(buffer)
  templateCache.set(template.id, bytes)
  return bytes
}

const loadTemplateInfo = async () => {
  if (!wasmReady.value || !wasmModule) return
  const zipBytes = await fetchTemplateZip()
  const info = wasmModule.template_info(zipBytes)
  templateInfo.value = info
}

const buildInputSpecs = async () => {
  const specs = [] as any[]
  for (const input of inputs.value) {
    if (input.kind === 'image') {
      if (!input.file) {
        throw new Error('Add a PNG file for every image input.')
      }
      const buffer = await input.file.arrayBuffer()
      specs.push({
        kind: 'image',
        layer: input.layer,
        bytes: Array.from(new Uint8Array(buffer)),
      })
    } else {
      if (!input.text.trim()) {
        throw new Error('Text inputs cannot be empty.')
      }
      specs.push({
        kind: 'text',
        layer: input.layer,
        text: input.text,
        font_size: input.fontSize,
        padding: input.padding,
        color: input.color || undefined,
        background: input.background || undefined,
      })
    }
  }
  return specs
}

const renderImage = async () => {
  if (!wasmReady.value || !wasmModule) return
  errorMessage.value = ''
  isRendering.value = true
  try {
    const zipBytes = await fetchTemplateZip()
    const inputSpecs = await buildInputSpecs()
    const options = {
      auto_zoom: autoZoom.value,
      width: outputWidth.value === '' ? undefined : Number(outputWidth.value),
      height: outputHeight.value === '' ? undefined : Number(outputHeight.value),
    }
    const output = wasmModule.render_gif(zipBytes, inputSpecs, options)
    if (previewUrl.value) URL.revokeObjectURL(previewUrl.value)
    const blob = new Blob([output], { type: 'image/gif' })
    previewUrl.value = URL.createObjectURL(blob)
  } catch (error: any) {
    errorMessage.value = error?.message ?? 'Render failed.'
  } finally {
    isRendering.value = false
  }
}

const downloadImage = () => {
  if (!previewUrl.value) return
  const link = document.createElement('a')
  link.href = previewUrl.value
  link.download = `${selectedTemplateId.value || 'maple'}.gif`
  document.body.appendChild(link)
  link.click()
  link.remove()
}

onMounted(async () => {
  await initWasm()
  await loadTemplateInfo()
})
</script>
