import type { HTMLAttributes } from 'react';

declare module 'react' {
  namespace JSX {
    interface IntrinsicElements {
      'esp-web-install-button': HTMLAttributes<HTMLElement>;
    }
  }
}
