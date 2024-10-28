use anyhow::Context;
use html5ever::{
    local_name, namespace_url, ns, parse_document, parse_fragment, serialize,
    tendril::{fmt::UTF8, Tendril, TendrilSink},
    Attribute, LocalName, QualName,
};
use markup5ever_rcdom::{Handle, Node, NodeData::Element, RcDom, SerializableHandle};
use std::{cell::RefCell, rc::Rc};

pub trait DOMBuilder {
    fn build_document(self) -> Result<RcDom, std::io::Error>;
    fn build_fragment(self) -> RcDom;
}

pub trait DOMOps {
    fn get_element_by_id(self, id: &str) -> Option<Rc<Node>>;
    fn get_head(self) -> Option<Rc<Node>>;
    fn find_meta_tags(self) -> Vec<Rc<Node>>;
}

pub trait NodeOps {
    fn get_attribute(&self, name: &LocalName) -> Option<Tendril<UTF8>>;
    fn set_attribute(&mut self, name: &LocalName, value: Tendril<UTF8>);
}

impl DOMBuilder for &str {
    fn build_document(self) -> Result<RcDom, std::io::Error> {
        parse_document(RcDom::default(), Default::default())
            .from_utf8()
            .read_from(&mut self.as_bytes())
    }

    fn build_fragment(self) -> RcDom {
        parse_fragment(
            RcDom::default(),
            Default::default(),
            QualName::new(None, ns!(html), local_name!("body")),
            vec![],
        )
        .one(self)
    }
}

impl DOMOps for Handle {
    fn get_element_by_id(self, id: &str) -> Option<Rc<Node>> {
        let children = self.children.borrow();
        for child in children.iter() {
            if let Element { ref attrs, .. } = &child.data {
                for attr in attrs.borrow().iter() {
                    if attr.name.local == local_name!("id") && attr.value == id.into() {
                        return Some(child.clone());
                    }
                }
            }

            if let Some(node) = Self::get_element_by_id(child.clone(), id) {
                return Some(node);
            }
        }

        None
    }

    fn get_head(self) -> Option<Rc<Node>> {
        let children = self.children.borrow();
        for child in children.iter() {
            if let Element { name, .. } = &child.data {
                if name.local == local_name!("head") {
                    return Some(child.clone());
                }

                if let Some(node) = Self::get_head(child.clone()) {
                    return Some(node);
                }
            }
        }

        None
    }

    fn find_meta_tags(self) -> Vec<Rc<Node>> {
        let mut meta_tags = Vec::new();
        let children = self.children.borrow();
        for child in children.iter() {
            if let Element { name, .. } = &child.data {
                if name.local == local_name!("meta") {
                    meta_tags.push(child.clone());
                }

                meta_tags.append(&mut Self::find_meta_tags(child.clone()));
            }
        }

        meta_tags
    }
}

impl NodeOps for Rc<Node> {
    fn get_attribute(&self, name: &LocalName) -> Option<Tendril<UTF8>> {
        if let Element { ref attrs, .. } = &self.data {
            for attr in attrs.borrow().iter() {
                if &attr.name.local == name {
                    return Some(attr.value.clone());
                }
            }
        }

        None
    }

    fn set_attribute(&mut self, name: &LocalName, value: Tendril<UTF8>) {
        if let Element { ref attrs, .. } = &self.data {
            for attr in attrs.borrow_mut().iter_mut() {
                if &attr.name.local == name {
                    attr.value = value;
                    return;
                }
            }
        }
    }
}

pub fn extract_contents(handle: &Handle) -> Vec<Rc<Node>> {
    let node: &Rc<Node> = handle;
    let children = node.children.borrow();
    if let Some(child) = children.iter().next() {
        match &child.data {
            Element { ref name, .. } => {
                if name.local == local_name!("html") {
                    child.children.borrow().iter().cloned().collect()
                } else {
                    extract_contents(child)
                }
            }
            _ => extract_contents(child),
        }
    } else {
        vec![]
    }
}

// todo: 转换为通用函数生成，此 API 作为快捷方式。
pub fn build_script(url: Tendril<UTF8>) -> Rc<Node> {
    Node::new(Element {
        name: QualName::new(None, ns!(html), local_name!("script")),
        attrs: RefCell::new(vec![Attribute {
            name: QualName::new(None, ns!(), local_name!("src")),
            value: url,
        }]),
        template_contents: RefCell::new(None),
        mathml_annotation_xml_integration_point: false,
    })
}

pub fn build_newline() -> Rc<Node> {
    Node::new(markup5ever_rcdom::NodeData::Text {
        contents: RefCell::new("\n".into()),
    })
}

pub fn serialize_to_html(dom: RcDom) -> anyhow::Result<String> {
    let mut buf = Vec::new();
    let document: SerializableHandle = dom.document.clone().into();
    serialize(&mut buf, &document, Default::default()).context("failed to serialize HTML")?;

    String::from_utf8(buf).context("failed to convert HTML to string")
}

#[cfg(test)]
mod dom_builder_tests {
    use super::*;
    use markup5ever_rcdom::NodeData::Document;

    #[test]
    fn test_build_document() {
        let html = r#"
            <html>
                <head>
                    <title>Test</title>
                </head>
                <body>
                    <div>
                        <p>Hello, World!</p>
                    </div>
                </body>
            </html>"#;

        let dom = html.build_document().unwrap();
        assert!(matches!(dom.document.clone().data, Document { .. }));
    }

    #[test]
    fn test_build_fragment() {
        let html = r#"
            <div>
                <p>Hello, World!</p>
            </div>"#;

        let dom = html.build_fragment();
        assert!(matches!(dom.document.clone().data, Document { .. }));
    }
}

#[cfg(test)]
mod dom_opts_tests {
    use super::*;
    use html5ever::QualName;

    #[test]
    fn test_get_element_by_id() {
        use html5ever::QualName;

        let html = r#"
            <html>
                <head>
                    <title>Test</title>
                </head>
                <body>
                    <div id="hello">
                        <p>Hello, World!</p>
                    </div>
                </body>
            </html>"#;

        let dom = html.build_document().unwrap();
        let result = dom.document.clone().get_element_by_id("hello");
        assert!(result.is_some());
        assert!(matches!(
            result.unwrap().data,
            Element {
                name: QualName {
                    local: local_name!("div"),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn test_get_head() {
        let html = r#"
            <html>
                <head><title>Test title</title></head>
                <body>
                    <div>
                        <p>Hello, World!</p>
                    </div>
                </body>
            </html>"#;

        let dom = html.build_document().unwrap();
        let result = dom.document.clone().get_head();
        assert!(result.is_some());
        assert!(matches!(
            result.clone().unwrap().data,
            Element {
                name: QualName {
                    local: local_name!("head"),
                    ..
                },
                ..
            }
        ));
        assert_eq!(result.unwrap().children.borrow().len(), 1);
    }

    #[test]
    fn test_find_meta_tags() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:description" content="Some description...">
                    <meta property="og:locale" content="zh-CN">
                    <meta property="og:site_name" content="Site Name">
                    <meta property="og:title" content="Some title... | Site Name">
                    <meta property="og:type" content="article">
                    <meta property="og:url" content="http://...">
                    <meta property="article:modified_time" content="2024-10-24T05:36:47+08:00">
                    <title>Test</title>
                </head>
                <body>
                    <div>
                        <meta property="custom" content="custom/non-standard locations">
                        <p>Hello, World!</p>
                    </div>
                </body>
            </html>"#;

        let dom = html.build_document().unwrap();
        let meta_tags = dom.document.clone().find_meta_tags();
        assert!(meta_tags.len() == 8);
    }
}

#[cfg(test)]
mod node_ops_tests {
    use super::*;

    #[test]
    fn test_get_attribute() {
        let html = r#"
            <html>
                <head>
                    <title>Test</title>
                </head>
                <body>
                    <div id="hello">
                        <p>Hello, World!</p>
                    </div>
                </body>
            </html>"#;

        let dom = html.build_document().unwrap();
        let div = dom.document.clone().get_element_by_id("hello").unwrap();
        let id = div.get_attribute(&local_name!("id"));
        assert!(id.is_some());
        assert_eq!(id.unwrap(), "hello".into());
    }

    #[test]
    fn test_set_attribute() {
        let html = r#"
            <html>
                <head>
                    <title>Test</title>
                </head>
                <body>
                    <div id="hello">
                        <p>Hello, World!</p>
                    </div>
                </body>
            </html>"#;

        let dom = html.build_document().unwrap();
        let mut div = dom.document.clone().get_element_by_id("hello").unwrap();
        div.set_attribute(&local_name!("id"), "world".into());
        let id = div.get_attribute(&local_name!("id"));
        assert!(id.is_some());
        assert_eq!(id.unwrap(), "world".into());
    }
}

#[test]
fn test_extract_contents() {
    let html =
        "<html><head><title>Test</title></head><body><div><p>Hello, World!</p></div></body></html>";

    let dom = html.build_document().unwrap();
    let contents = extract_contents(&dom.document);
    assert!(matches!(
        contents[0].data,
        Element {
            name: QualName {
                local: local_name!("head"),
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        contents[1].data,
        Element {
            name: QualName {
                local: local_name!("body"),
                ..
            },
            ..
        }
    ));
    assert_eq!(contents.len(), 2);
}

#[test]
fn test_serialize_to_html() {
    use std::cell::RefCell;

    let html =
        "<html><head><title>Test</title></head><body><div><p id=\"hello\">Hello, World!</p></div></body></html>";

    let dom = html.build_document().unwrap();
    if let Some(hello_node) = dom.document.clone().get_element_by_id("hello") {
        hello_node
            .children
            .replace(vec![Node::new(markup5ever_rcdom::NodeData::Text {
                contents: RefCell::new("Good bye!".into()),
            })]);
    }
    let result = serialize_to_html(dom);
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        "<html><head><title>Test</title></head><body><div><p id=\"hello\">Good bye!</p></div></body></html>"
    );
}
