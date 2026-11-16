//! Pure merge of single PDFs into one document, pages in the given order.

use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};

use crate::errors::Result;

/// Page attributes a page may inherit from its page tree; copied onto the page itself before
/// the source page tree is dropped.
const INHERITED: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// Object types that belong to a source document's structure and are rebuilt or dropped.
const DROPPED: [&[u8]; 4] = [b"Catalog", b"Pages", b"ObjStm", b"XRef"];

fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    // Walks up the Parent chain; the bound stops a malformed cyclic tree.
    std::iter::successors(Some(page), |id| {
        doc.get_dictionary(*id)
            .ok()?
            .get(b"Parent")
            .ok()?
            .as_reference()
            .ok()
    })
    .take(64)
    .find_map(|id| doc.get_dictionary(id).ok()?.get(key).ok().cloned())
}

fn materialize(doc: &Document, page: ObjectId) -> Result<Dictionary> {
    let mut dict = doc.get_dictionary(page)?.clone();
    INHERITED
        .iter()
        .filter_map(|key| inherited(doc, page, key).map(|value| (*key, value)))
        .for_each(|(key, value)| dict.set(key, value));
    Ok(dict)
}

/// Prepares one source document: pages carry their own attributes and point to `parent`;
/// object ids start after `first_free - 1`. Returns its page ids and objects to keep.
fn prepare(
    mut doc: Document,
    first_free: u32,
    parent: ObjectId,
) -> Result<(Vec<ObjectId>, Document)> {
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let materialized = pages
        .iter()
        .map(|id| materialize(&doc, *id).map(|dict| (*id, dict)))
        .collect::<Result<Vec<_>>>()?;
    materialized.into_iter().for_each(|(id, dict)| {
        doc.objects.insert(id, Object::Dictionary(dict));
    });
    doc.renumber_objects_with(first_free);
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    doc.objects.retain(|_, object| {
        object
            .type_name()
            .map_or(true, |name| !DROPPED.contains(&name))
    });
    pages.iter().for_each(|id| {
        if let Some(Object::Dictionary(page)) = doc.objects.get_mut(id) {
            page.set("Parent", parent);
        }
    });
    Ok((pages, doc))
}

/// Merges `pdfs` into one PDF, keeping every page in order.
pub(crate) fn merge_pdfs(pdfs: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut merged = Document::with_version("1.7");
    let pages_id = merged.new_object_id();
    let kids = pdfs
        .iter()
        .try_fold(Vec::new(), |mut kids, bytes| -> Result<Vec<ObjectId>> {
            let source = Document::load_mem(bytes)?;
            let (pages, doc) = prepare(source, merged.max_id + 1, pages_id)?;
            merged.max_id = merged.max_id.max(doc.max_id);
            merged.objects.extend(doc.objects);
            kids.extend(pages);
            Ok(kids)
        })?;
    let count = i64::try_from(kids.len()).unwrap_or(i64::MAX);
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => count,
        }),
    );
    let catalog = merged.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    merged.trailer.set("Root", catalog);
    merged.compress();
    let mut bytes = Vec::new();
    merged.save_to(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::Error;

    /// A one-page PDF whose MediaBox is inherited from the page tree, marked with `label`.
    fn single(label: &str, width: i64) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = doc.add_object(lopdf::Stream::new(
            dictionary! {},
            label.as_bytes().to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page)],
                "Count" => 1,
                "MediaBox" => vec![0.into(), 0.into(), width.into(), 100.into()],
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn page_width(doc: &Document, page: ObjectId) -> i64 {
        doc.get_dictionary(page)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap()[2]
            .as_i64()
            .unwrap()
    }

    #[test]
    fn keeps_pages_in_order_with_their_attributes() {
        let merged = merge_pdfs(&[
            single("first", 10),
            single("second", 20),
            single("third", 30),
        ])
        .unwrap();
        let doc = Document::load_mem(&merged).unwrap();
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        assert_eq!(pages.len(), 3);
        let widths: Vec<i64> = pages.iter().map(|p| page_width(&doc, *p)).collect();
        assert_eq!(widths, [10, 20, 30]);
        let contents: Vec<Vec<u8>> = pages.iter().map(|p| doc.get_page_content(*p)).collect();
        // lopdf ends each content stream it returns with a newline.
        assert_eq!(
            contents,
            [
                b"first\n".to_vec(),
                b"second\n".to_vec(),
                b"third\n".to_vec()
            ]
        );
    }

    #[test]
    fn single_input_gives_one_page() {
        let doc = Document::load_mem(&merge_pdfs(&[single("only", 10)]).unwrap()).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn invalid_input_is_an_error() {
        assert!(matches!(
            merge_pdfs(&[b"not a pdf".to_vec()]),
            Err(Error::Merge(_))
        ));
    }
}
