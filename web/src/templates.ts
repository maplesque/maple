export type TemplateEntry = {
  id: string
  name: string
  file: string
  vibe: string
}

export const templates: TemplateEntry[] = [
  { id: 'back-tattoo', name: 'Back Tattoo', file: 'back-tattoo.zip', vibe: 'Ink that moves.' },
  { id: 'billboard-cityscape', name: 'Billboard Cityscape', file: 'billboard-cityscape.zip', vibe: 'Late-night neon ads.' },
  { id: 'book', name: 'Book Jacket', file: 'book.zip', vibe: 'Front + back cover flair.' },
  { id: 'circuitboard', name: 'Circuit Board', file: 'circuitboard.zip', vibe: 'Electric etching.' },
  { id: 'flag', name: 'Flag', file: 'flag.zip', vibe: 'Wind-swept banner.' },
  { id: 'flag2', name: 'Flag Variant', file: 'flag2.zip', vibe: 'Sharper folds.' },
  { id: 'fortune-cookie', name: 'Fortune Cookie', file: 'fortune-cookie.zip', vibe: 'Crack it open.' },
  { id: 'heart-locket', name: 'Heart Locket', file: 'heart-locket.zip', vibe: 'Keepsake glow.' },
  { id: 'rubiks', name: 'Rubiks', file: 'rubiks.zip', vibe: 'Twist the cube.' },
  { id: 'toaster', name: 'Toaster', file: 'toaster.zip', vibe: 'Pop-up nostalgia.' },
  { id: 'valentine', name: 'Valentine', file: 'valentine.zip', vibe: 'Sweet reveal.' },
]
