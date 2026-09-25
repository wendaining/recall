import { en } from './en'

type WidenStrings<T> = {
  [K in keyof T]: T[K] extends string ? string : WidenStrings<T[K]>
}

export type Messages = WidenStrings<typeof en>
export type Locale = 'en'

const dictionaries: Record<Locale, Messages> = { en }

export function getMessages(locale: Locale = 'en'): Messages {
  return dictionaries[locale]
}
