<template>
  <div class="min-h-screen px-4 py-8 sm:px-6 sm:py-10">
    <div class="max-w-6xl mx-auto space-y-10">
      <header class="flex flex-col gap-6 md:flex-row md:items-end md:justify-between fade-up">
        <div class="space-y-4">
          <div class="badge">
            <span class="i-lucide-sparkles"></span>
            WASM template compositor
          </div>
          <div class="space-y-2">
            <h1 class="text-3xl sm:text-4xl md:text-5xl font-semibold tracking-tight">
              Maple Studio
            </h1>
            <p class="text-slate-300 md:text-slate-400 max-w-2xl">
              Generate PNG artwork from Maple templates in the browser. Choose a bundled template, drop in PNG layers
              or text, and render everything locally with WebAssembly.
            </p>
          </div>
        </div>
        <div class="flex flex-wrap gap-3">
          <div class="badge">
            <span class="i-lucide-moon"></span>
            Dark mode only
          </div>
          <div class="badge">
            <span class="i-lucide-image"></span>
            PNG inputs only
          </div>
        </div>
      </header>

      <main class="grid gap-8 lg:grid-cols-[1.1fr_0.9fr]">
        <section class="space-y-6">
          <div class="card p-6 space-y-5">
            <div class="flex items-center justify-between">
              <div>
                <h2 class="text-lg font-semibold">Templates</h2>
                <p class="text-sm text-slate-300 md:text-slate-400">Bundled from the repo. All local.</p>
              </div>
              <div v-if="templateInfo" class="text-sm md:text-xs text-slate-300 md:text-slate-400 text-right">
                <div>{{ templateInfo.width }} × {{ templateInfo.height }}</div>
                <div>{{ templateInfo.frames }} frame<span v-if="templateInfo.frames !== 1">s</span></div>
              </div>
            </div>
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              <button
                v-for="template in templates"
                :key="template.id"
                class="text-left px-4 py-3 rounded-2xl border transition"
                :class="selectedTemplateId === template.id
                  ? 'border-[#ff8c42] bg-[#1a2332] glow-ring'
                  : 'border-[#1f2a3b] bg-[#0f1624] hover:border-[#3a4a63]'"
                @click="selectTemplate(template.id)"
              >
                <div class="text-sm text-slate-200">{{ template.name }}</div>
                <div class="text-sm md:text-xs text-slate-400">{{ template.vibe }}</div>
              </button>
            </div>
            <div class="flex items-center gap-3 text-sm md:text-xs text-slate-400">
              <span class="i-lucide-folder-open"></span>
              {{ selectedTemplate?.file }}
            </div>
          </div>

          <div class="card p-6 space-y-5">
            <div class="flex items-center justify-between">
              <div>
                <h2 class="text-lg font-semibold">Inputs</h2>
                <p class="text-sm text-slate-300 md:text-slate-400">Mix PNG layers and text blocks. Layer numbers map to template masks.</p>
              </div>
              <div class="text-sm md:text-xs text-slate-400">{{ inputs.length }} input{{ inputs.length !== 1 ? 's' : '' }}</div>
            </div>

            <div class="space-y-4">
              <div v-for="(input, index) in inputs" :key="input.id" class="card-soft p-4 space-y-3">
                <div class="flex items-center justify-between">
                  <div class="text-sm font-semibold">Input {{ index + 1 }}</div>
                  <button
                    class="text-xs text-slate-400 hover:text-[#ff8c42]"
                    @click="removeInput(input.id)"
                    :disabled="inputs.length === 1"
                  >
                    Remove
                  </button>
                </div>

                <div class="grid gap-3 md:grid-cols-2">
                  <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Type</label>
                  <div class="input-shell">
                    <select v-model="input.kind" class="w-full bg-transparent text-sm">
                      <option value="image">PNG image</option>
                      <option value="text">Text</option>
                    </select>
                  </div>

                  <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Layer</label>
                  <div class="input-shell">
                    <input v-model.number="input.layer" type="number" min="1" max="255" class="w-full bg-transparent text-sm" />
                  </div>
                </div>

                <div v-if="input.kind === 'image'" class="space-y-2">
                  <div class="input-shell">
                    <input
                      type="file"
                      accept="image/png"
                      class="w-full text-sm"
                      @change="(event) => onFileChange(event, input)"
                    />
                  </div>
                  <div class="text-sm md:text-xs text-slate-400">PNG only. Files never leave your browser.</div>
                  <div v-if="input.file" class="text-sm md:text-xs text-slate-300">{{ input.file.name }}</div>
                </div>

                <div v-else class="space-y-3">
                  <div class="input-shell">
                    <textarea
                      v-model="input.text"
                      rows="2"
                      placeholder="Enter text"
                      class="w-full bg-transparent text-sm resize-none"
                    ></textarea>
                  </div>
                  <div class="grid gap-3 md:grid-cols-2">
                    <div>
                      <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Font size</label>
                      <div class="input-shell mt-1">
                        <input v-model.number="input.fontSize" type="number" min="10" max="240" class="w-full bg-transparent text-sm" />
                      </div>
                    </div>
                    <div>
                      <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Padding</label>
                      <div class="input-shell mt-1">
                        <input v-model.number="input.padding" type="number" min="0" max="200" class="w-full bg-transparent text-sm" />
                      </div>
                    </div>
                    <div>
                      <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Text color</label>
                      <div class="input-shell mt-1">
                        <input v-model="input.color" type="text" placeholder="#111111" class="w-full bg-transparent text-sm" />
                      </div>
                    </div>
                    <div>
                      <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Background</label>
                      <div class="input-shell mt-1">
                        <input v-model="input.background" type="text" placeholder="#ffffff" class="w-full bg-transparent text-sm" />
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <div class="flex flex-wrap gap-3">
              <button class="px-4 py-2 rounded-full bg-[#1f2a3b] text-sm hover:border-[#3a4a63] border border-transparent" @click="addInput('image')">
                + Image input
              </button>
              <button class="px-4 py-2 rounded-full bg-[#1f2a3b] text-sm hover:border-[#3a4a63] border border-transparent" @click="addInput('text')">
                + Text input
              </button>
            </div>
          </div>

          <div class="card p-6 space-y-4">
            <div class="flex items-center justify-between">
              <div>
                <h2 class="text-lg font-semibold">Render settings</h2>
                <p class="text-sm text-slate-300 md:text-slate-400">Tune fit and output size.</p>
              </div>
              <div v-if="templateInfo && templateInfo.frames > 1" class="text-sm md:text-xs text-slate-400">
                Animation · {{ templateInfo.frames }} frames
              </div>
            </div>

            <div v-if="templateInfo && templateInfo.frames > 1" class="text-sm text-slate-400">
              Full animation renders as a GIF.
            </div>

            <div class="grid gap-3 md:grid-cols-2">
              <div class="flex items-center gap-2">
                <input id="autoZoom" v-model="autoZoom" type="checkbox" class="accent-[#ff8c42]" />
                <label for="autoZoom" class="text-sm text-slate-200">Auto-fit inputs</label>
              </div>
              <div class="text-sm md:text-xs text-slate-400 md:text-right">Keeps content centered in template</div>
            </div>

            <div class="grid gap-3 md:grid-cols-2">
              <div>
                <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Output width</label>
                <div class="input-shell mt-1">
                  <input v-model="outputWidth" type="number" min="1" placeholder="Auto" class="w-full bg-transparent text-sm" />
                </div>
              </div>
              <div>
                <label class="text-sm md:text-xs text-slate-300 md:text-slate-400">Output height</label>
                <div class="input-shell mt-1">
                  <input v-model="outputHeight" type="number" min="1" placeholder="Auto" class="w-full bg-transparent text-sm" />
                </div>
              </div>
            </div>

            <button
              class="w-full mt-2 px-4 py-3 rounded-2xl text-sm font-semibold tracking-wide"
              :class="isRendering ? 'bg-[#2a3446] text-slate-400' : 'bg-[#ff8c42] text-black hover:brightness-110'"
              :disabled="isRendering || !wasmReady"
              @click="renderImage"
            >
              <span v-if="!wasmReady">WASM build missing — run npm run build:wasm</span>
              <span v-else-if="isRendering">Rendering...</span>
              <span v-else>Render GIF</span>
            </button>
          </div>
        </section>

        <section class="space-y-6">
          <div class="card p-6 space-y-4">
            <div class="flex items-center justify-between">
              <div>
                <h2 class="text-lg font-semibold">Preview</h2>
                <p class="text-sm text-slate-300 md:text-slate-400">Latest render result.</p>
              </div>
              <button
                v-if="previewUrl"
                class="text-sm md:text-xs text-slate-300 hover:text-[#36d1b7]"
                @click="downloadImage"
              >
                Download GIF
              </button>
            </div>

            <div class="preview-frame">
              <img v-if="previewUrl" :src="previewUrl" alt="Rendered preview" class="w-full h-full object-contain" />
              <div v-else class="text-sm text-slate-400">No render yet. Generate a GIF.</div>
            </div>
          </div>

          <div class="card-soft p-4 space-y-2">
            <div class="text-sm md:text-xs text-slate-400">Status</div>
            <div v-if="errorMessage" class="text-sm text-[#ff8c42]">{{ errorMessage }}</div>
            <div v-else class="text-sm text-slate-200">
              {{ wasmReady ? 'WASM ready. Render stays local.' : 'WASM not built yet.' }}
            </div>
          </div>
        </section>
      </main>
    </div>
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

const initWasm = async () => {
  try {
    const module = await import('./wasm/maple.js')
    await module.default()
    wasmModule = module
    wasmReady.value = true
  } catch (error) {
    wasmReady.value = false
    errorMessage.value = 'WASM not found. Run npm run build:wasm.'
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
