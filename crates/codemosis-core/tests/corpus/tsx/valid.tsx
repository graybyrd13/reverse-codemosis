// MARK_COMMENT
const GREETING = "MARK_STRING";

export function Greeting(props: { name: string }) {
  return <p>{GREETING}, {props.name}</p>;
}
