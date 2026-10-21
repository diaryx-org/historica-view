// The mail the hub sends: a link to verify an address, and a link to choose
// a new password. Both go through Cloudflare Email Sending's `send_email`
// binding, which is the credential, so there is no API key to hold.

/** A message, as the `send_email` binding's builder form takes one. */
export interface Message {
  from: { name: string; email: string };
  to: string;
  subject: string;
  text: string;
  html: string;
}

/**
 * The `send_email` binding. Declared here because the workers-types this
 * worker pins knows only the older `send(EmailMessage)`, a raw MIME message.
 */
export interface SendEmail {
  send(message: Message): Promise<unknown>;
}

/** Sends one message to one address. */
export type Mailer = (message: Omit<Message, "from">) => Promise<void>;

/** A mailer over `binding`, from `email` under the hub's name. */
export function mailer(binding: SendEmail, email: string): Mailer {
  return async (message) => {
    await binding.send({ from: { name: "historica hub", email }, ...message });
  };
}

/** The message that verifies an address, with the link that does it. */
export function verification(to: string, url: string): Omit<Message, "from"> {
  return link(
    to,
    "Verify your email for historica hub",
    "Open this link to verify your email and finish making your account:",
    url,
    "The link works for an hour. If you did not make an account, ignore this message.",
  );
}

/** The message that resets a password, with the link that does it. */
export function reset(to: string, url: string): Omit<Message, "from"> {
  return link(
    to,
    "Reset your historica hub password",
    "Open this link to choose a new password:",
    url,
    "The link works for an hour. If you did not ask for this, ignore this message: your password stays as it is.",
  );
}

function link(
  to: string,
  subject: string,
  lead: string,
  url: string,
  after: string,
): Omit<Message, "from"> {
  const text = `${lead}\n\n${url}\n\n${after}\n`;
  const html =
    `<p>${escape(lead)}</p>\n` +
    `<p><a href="${escape(url)}">${escape(url)}</a></p>\n` +
    `<p>${escape(after)}</p>\n`;
  return { to, subject, text, html };
}

function escape(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
