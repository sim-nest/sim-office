export type OutlookSelectedItem = {
  itemId: string;
  subject?: string;
  itemType?: string;
};

declare const Office: any;

export async function selectedItem(): Promise<OutlookSelectedItem> {
  const item = Office.context.mailbox.item;
  return {
    itemId: item.itemId,
    subject: item.subject,
    itemType: item.itemType,
  };
}
