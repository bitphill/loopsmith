# Hyperframes Composition Brief: loopsmith

## Objective
Create a 20s voiced launch brag for loopsmith — skill-craft manifesto tone using the site’s forge palette and verbatim product copy.

## Output
- Composition directory: `brag-output/composition/`
- Rendered video: `brag-output/brag.mp4`
- Format: landscape — 1920x1080
- Duration: 20 seconds

## Source Material
- Project root: `/Users/vaibhav/projects/workspace/loopsmith`
- Primary files: `site/index.html`, `README.md`
- Product name: loopsmith
- Tagline / strongest claim: header tagline + hero headline + gate rule
- Key UI moment: terminal loop + PASS gate
- Copy verbatim:
  - The skill to create self evolving loops based purpose specific skills
  - Describe a job once. It runs until it is actually done.
  - the model never gets to say "done"
  - The gate is compiled code

## Creative Direction
- Tone preset: app-store
- Creative direction: The skill to create self evolving loop based purpose specific skills
- Interpretation: Earnest skill manifesto; forge ember + quench verify colors; narration-led
- Angle: Purpose-specific skills become self-evolving loops with a code gate
- Hook: ember tagline
- Outro: loopsmith logo
- Avoid: generic SaaS filler, invented feature claims

## Visual Identity
- Background: #faf7f2
- Text: #191410
- Accent: #c2410c
- Quench: #0e7490
- Display font: system-ui stack
- Body font: system-ui stack
- References: landing header tagline, hero, fact cards, logo

## Storyboard
See `brag-output/brag-plan.md` — 5 scenes, 20s total.

## Audio
- Music: happy-beats-business-moves-vol-1-by-ende-dot-app.mp3 (duck under VO)
- Voice: Kokoro af_heart → assets/voiceover.wav on track 3
- SFX: click on PASS, soft impact on hero, select on cards
- Music cue guidance: bundled preset; beat-lock ~3.02s; card grid ~12.65s+
- Audio-reactive: subtle ember glow only

## Hyperframes Instructions
Single root composition `main`, GSAP timeline, class="clip" on timed visuals, run check then render.
