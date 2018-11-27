#!/bin/bash

encodings="utf8 utf16le utf16be"

utf8_bom='\xEF\xBB\xBF'
utf16le_bom='\xFF\xFE'
utf16be_bom='\xFE\xFF'

for encoding in ${encodings}; do
	bomvar="${encoding}_bom"

	echo "Converting validation/${encoding}.xml into ${encoding}/doc.xml"
	iconv -f utf8 -t ${encoding} validation/${encoding}.xml > ${encoding}/doc.xml
	echo "	Constructing BOM variant..."
	echo -ne "${!bomvar}" > ${encoding}_bom/doc.xml
	cat ${encoding}/doc.xml >> ${encoding}_bom/doc.xml

	echo "Converting validation/${encoding}_xmldecl.xml into ${encoding}/doc_xmldecl.xml"
	iconv -f utf8 -t ${encoding} validation/${encoding}_xmldecl.xml > ${encoding}/doc_xmldecl.xml
	echo "	Constructing BOM variant..."
	echo -ne "${!bomvar}" > ${encoding}_bom/doc_xmldecl.xml
	cat ${encoding}/doc_xmldecl.xml >> ${encoding}_bom/doc_xmldecl.xml

	echo "Converting validation/${encoding}_xmldecl_encodingdecl.xml into ${encoding}/doc_xmldecl_encodingdecl.xml"
	iconv -f utf8 -t ${encoding} validation/${encoding}_xmldecl_encodingdecl.xml > ${encoding}/doc_xmldecl_encodingdecl.xml
	echo "	Constructing BOM variant..."
	echo -ne "${!bomvar}" > ${encoding}_bom/doc_xmldecl_encodingdecl.xml
	cat ${encoding}/doc_xmldecl_encodingdecl.xml >> ${encoding}_bom/doc_xmldecl_encodingdecl.xml
done
