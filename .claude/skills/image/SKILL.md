---
name: image
description: Generate, edit, and compose images using Google's Gemini image models via cURL and the REST API. Use this skill when the user asks to create images, generate visuals, edit photos, compose multiple images, create logos, thumbnails, infographics, product shots, or any image generation task. Supports text-to-image, image editing, multi-image composition, iterative refinement, and aspect ratio control.
user-invocable: true
---

# Image

Image generation skill powered by Google's Gemini image models via the REST API.

## Prerequisites

- `GEMINI_API_KEY` environment variable must be exported (billing-enabled key required)
- `curl`, `jq`, `cwebp` (from libwebp) on PATH
- macOS: `brew install jq libwebp` (curl is built-in)
- Linux: `apt install jq webp curl`

## Shell Commands

All commands live in this skill's directory. They find each other automatically via `SCRIPT_DIR` — no PATH setup needed. Run them with their full path or add the directory to PATH.

### Text-to-Image Generation

```bash
.claude/skills/image/create_image <prompt> <output_image> [aspect_ratio] [image_size]
```

Examples:

```bash
.claude/skills/image/create_image "A cat wearing a wizard hat" output.webp
.claude/skills/image/create_image "Futuristic motorcycle on Mars" output.webp "16:9"
.claude/skills/image/create_image "Futuristic motorcycle on Mars" output.webp "16:9" "4K"
```

Default image size is `2K`.

### Edit an Existing Image

```bash
.claude/skills/image/modify_image <source_image> <prompt> <output_image> [aspect_ratio]
```

Examples:

```bash
.claude/skills/image/modify_image input.png "Add a sunset to the background" output.webp
.claude/skills/image/modify_image input.png "Add a sunset to the background" output.webp "16:9"
```

Supports PNG, WebP, and JPEG source images.

### Compose Multiple Images

```bash
.claude/skills/image/compose_image <prompt> <output_image> <source_image>...
```

Supports up to 14 reference images in PNG, WebP, or JPEG format.

Examples:

```bash
.claude/skills/image/compose_image "Create a group photo in an office setting" output.webp person1.png person2.png
.claude/skills/image/compose_image "Combine these into a collage" collage.webp photo1.jpg photo2.png photo3.webp
```

## Generation Options

### Aspect Ratios

`1:1`, `2:3`, `3:2`, `3:4`, `4:3`, `4:5`, `5:4`, `9:16`, `16:9`, `21:9`

### Resolutions

`1K` (1024px), `2K` (default), `4K`

## Prompting Tips

**Photorealistic**: Include camera settings, lighting, lens details
```
"Shot on 85mm lens, golden hour lighting, shallow depth of field"
```

**Logos**: Specify style, colors, typography
```
"Clean minimalist logo, sans-serif font, monochrome, vector style"
```

**Product shots**: Describe studio setup
```
"Studio-lit, 3-point softbox, polished surface, 45-degree angle"
```

**Stylized art**: Name the style explicitly
```
"Anime style, cel-shading, bold outlines, vibrant colors"
```

## Error Handling

- **Missing API key**: Ensure `GEMINI_API_KEY` is exported
- **Empty output file**: Safety filters may have blocked the prompt — check for `blockReason` in stderr
- **Large images for editing**: Very large source images may exceed request size limits — resize before encoding
- **Quota errors (429)**: Free-tier quotas may be 0 — a billing-enabled API key is required

## How to Use This Skill

When the user invokes `/image`, interpret `$ARGUMENTS` as the image generation task. Determine the appropriate workflow based on the request and use the corresponding command: `create_image` for text-to-image generation, `modify_image` for editing an existing image, or `compose_image` for combining multiple images. Always save output images to the current working directory unless the user specifies a different path.
