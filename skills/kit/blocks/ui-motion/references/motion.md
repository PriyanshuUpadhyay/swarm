# Motion tables

These tables are the timing owner when the code has no duration scale or easing tokens of its own.
They come from Emil Kowalski's `animate` skill (https://github.com/emilkowalski/skills at
e8a175de22ae1e49370fc144c1f3bb9aeedf988d), MIT License, Copyright (c) 2026 Emil Kowalski.

## Animate at all

| How often the user sees it | Decision |
|---|---|
| 100 or more times a day, or from a keyboard shortcut | No animation |
| Tens of times a day (hover, list navigation) | Fast and subtle, or none |
| Sometimes (modal, drawer, toast) | Standard animation |
| Rare or first time (onboarding, success) | Delight is allowed |

Name the purpose before you build: feedback, where a thing came from or went, a state change,
a bridge for content that would jump, or an explanation. Data that the user reads does not move for style.

## Easing

| Case | Easing |
|---|---|
| Enter or exit | `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)` |
| Move or change shape on screen | `--ease-in-out: cubic-bezier(0.77, 0, 0.175, 1)` |
| Drawer | `--ease-drawer: cubic-bezier(0.32, 0.72, 0, 1)` |
| Hover or color change | `ease` |
| Constant motion (marquee, progress) | `linear` |

Never use `ease-in` on UI. For drag with momentum or a gesture that the user can reverse, use a
spring: `{ type: "spring", duration: 0.5, bounce: 0.2 }`, with bounce from 0.1 to 0.3.

## Duration

| Element | Duration |
|---|---|
| Button press | 100-160 ms |
| Tooltip, small popover | 125-200 ms |
| Dropdown, select | 150-250 ms |
| Modal, drawer | 200-500 ms |
| Stagger between list items | 30-80 ms |

## Interruption and exit

- Use a transition, not keyframes, for a thing that the user can start twice in a second (toast,
  toggle). A transition starts from the current value; keyframes start again from zero.
- Exit on the same path as the entry.
- Make the part where the user decides slow and the system's answer fast. Example: hold-to-confirm
  takes 2 s `linear`, and the release takes 200 ms `ease-out`.

## Hover and properties

- Animate `transform` and `opacity`. `clip-path` is also allowed, and `height` only for an accordion.
- Put hover motion inside `@media (hover: hover) and (pointer: fine)`, because a tap on a touch
  screen fires a false hover.
- Start an entrance from `scale(0.95)` and `opacity: 0`, never from `scale(0)`.
- Set `transform-origin` at the trigger for a popover, dropdown, menu, or tooltip. A modal stays centered.
- In Motion (`motion.dev`), use the full `transform` string, not the `x`, `y`, or `scale` props.

## Never ship

| Never | Use |
|---|---|
| `transition: all` | The exact properties |
| `scale(0)` entrance | `scale(0.95)` and `opacity: 0` |
| `ease-in` on UI | `--ease-out` |
| A UI duration over 300 ms with no reason | 150-250 ms |
| Keyframes on a toast or toggle | A transition |
| Animated `width`, `height`, `margin`, `padding`, `top`, or `left` | `transform` or `opacity` |
| Hover motion with no pointer gate | The media query above |
| All items enter at the same time | A 30-80 ms stagger |
