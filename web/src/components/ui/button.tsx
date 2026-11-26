import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "../../lib/utils";

const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg text-base font-medium outline-none select-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:text-disabled-foreground aria-disabled:text-disabled-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5",
  {
    variants: {
      variant: {
        default: "bg-brand text-brand-foreground hover:bg-brand-hover active:bg-brand-pressed disabled:bg-brand-disabled aria-disabled:bg-brand-disabled",
        outline: "border border-border bg-transparent text-foreground hover:bg-quiet-hover active:bg-quiet-pressed",
        ghost: "bg-transparent text-foreground hover:bg-quiet-hover active:bg-quiet-pressed data-[state=open]:bg-quiet-hover",
        destructive: "border border-failed bg-transparent text-failed hover:bg-quiet-hover active:bg-quiet-pressed disabled:border-border",
      },
      size: {
        default: "h-8 px-3.5",
        sm: "h-8 px-2",
        icon: "size-7",
        "icon-sm": "size-6",
        send: "h-7 w-16",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

function Button({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: React.ComponentProps<"button"> & VariantProps<typeof buttonVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot : "button";
  return <Comp data-slot="button" className={cn(buttonVariants({ variant, size, className }))} {...props} />;
}

export { Button, buttonVariants };
